//! Orden `run`: ejecuta un programa en el modelo de procesador elegido con el stdin como entrada
//! de la consola, y cuenta por qué se paró, cómo quedaron los registros y, si se pide, la traza.

use std::io::{self, BufRead, Write};

use rv_asm::Program;
use rv_core::{
    AccessKind, FiveStage, Halt, Hazards, MapError, MemoryError, Processor, Retired, SingleCycle,
    Stats, Trap,
};
use rv_isa::{Reg, RegisterStyle};

/// Tope de ciclos por defecto: de sobra para los programas de clase y aun así un bucle
/// infinito termina en pocos segundos.
pub const DEFAULT_MAX_CYCLES: u64 = 10_000_000;

/// Modelos de procesador que se pueden elegir con `--cpu`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    SingleCycle,
    FiveStageNoHazards,
    FiveStageStalls,
    FiveStageForwarding,
}

impl Model {
    /// Nombre de cada modelo en la línea de comandos, en el orden en que se listan.
    pub const NAMES: [(&'static str, Model); 4] = [
        ("un-ciclo", Model::SingleCycle),
        ("5-etapas-sin-riesgos", Model::FiveStageNoHazards),
        ("5-etapas-burbujas", Model::FiveStageStalls),
        ("5-etapas-forwarding", Model::FiveStageForwarding),
    ];

    pub fn parse(name: &str) -> Option<Model> {
        Model::NAMES
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .map(|(_, model)| *model)
    }

    fn boot(self, program: &Program) -> Result<Box<dyn Processor>, MapError> {
        Ok(match self {
            Model::SingleCycle => Box::new(SingleCycle::new(program)?),
            Model::FiveStageNoHazards => Box::new(FiveStage::new(program, Hazards::Ignore)?),
            Model::FiveStageStalls => Box::new(FiveStage::new(program, Hazards::Stall)?),
            Model::FiveStageForwarding => Box::new(FiveStage::new(program, Hazards::Forward)?),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunOptions {
    pub model: Model,
    pub trace: bool,
    pub max_cycles: u64,
    pub style: RegisterStyle,
}

impl Default for RunOptions {
    fn default() -> RunOptions {
        RunOptions {
            model: Model::SingleCycle,
            trace: false,
            max_cycles: DEFAULT_MAX_CYCLES,
            style: RegisterStyle::Abi,
        }
    }
}

/// Cómo acabó la ejecución.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    Halted(Halt),
    CycleLimit,
}

impl Ending {
    /// Código de salida del proceso: el de la `ecall` de salida (sus 8 bits bajos, como en
    /// Unix), 0 en un `ebreak`, que en la terminal marca el final, y 1 si el programa falló o
    /// no terminó.
    pub fn exit_code(self) -> u8 {
        match self {
            Ending::Halted(Halt {
                trap: Trap::Exited { code },
                ..
            }) => (code & 0xff) as u8,
            Ending::Halted(Halt {
                trap: Trap::Breakpoint,
                ..
            }) => 0,
            Ending::Halted(_) | Ending::CycleLimit => 1,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Map(#[from] MapError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Ejecuta `program`. La salida del programa va a `out` según se produce (así una pregunta se
/// ve antes de teclear la respuesta) y la traza y el informe final a `report`, para que
/// redirigir stdout guarde solo lo que imprime el programa.
pub fn run_program(
    program: &Program,
    options: &RunOptions,
    input: &mut impl BufRead,
    out: &mut impl Write,
    report: &mut impl Write,
) -> Result<(Ending, Stats), RunError> {
    let mut cpu = options.model.boot(program)?;
    let mut printed = 0;
    let ending = loop {
        if let Some(halt) = cpu.halt() {
            break Ending::Halted(halt);
        }
        if cpu.stats().cycles >= options.max_cycles {
            break Ending::CycleLimit;
        }
        let retired = cpu.step();
        // La línea de la traza antes que lo que imprime esa misma ecall: así se lee en orden.
        if options.trace {
            for entry in &retired {
                write_trace_line(program, cpu.stats().cycles, entry, options.style, report)?;
            }
        }
        printed = flush_output(cpu.as_ref(), printed, out)?;
        if cpu.awaiting_input() {
            feed_line(cpu.as_mut(), input)?;
        }
    };
    out.flush()?;
    write_summary(program, cpu.as_ref(), ending, options.style, report)?;
    Ok((ending, cpu.stats()))
}

/// Escribe lo que el programa ha impreso desde la última vez y devuelve cuánto lleva escrito.
fn flush_output(cpu: &dyn Processor, printed: usize, out: &mut impl Write) -> io::Result<usize> {
    let output = cpu.state().console.output();
    if let Some(fresh) = output.get(printed..).filter(|fresh| !fresh.is_empty()) {
        out.write_all(fresh)?;
        out.flush()?;
    }
    Ok(output.len())
}

/// Da a la consola una línea más de la entrada; al acabarse, la cierra para que leer sea un
/// error en vez de una espera eterna.
fn feed_line(cpu: &mut dyn Processor, input: &mut impl BufRead) -> io::Result<()> {
    let mut line = String::new();
    if input.read_line(&mut line)? == 0 {
        cpu.console_mut().close_input();
    } else {
        cpu.console_mut().push_input(&line);
    }
    Ok(())
}

fn line_label(program: &Program, pc: u32) -> String {
    program
        .line_at(pc)
        .map_or_else(|| "     ".to_string(), |line| format!("L{line:<4}"))
}

fn write_trace_line(
    program: &Program,
    cycle: u64,
    retired: &Retired,
    style: RegisterStyle,
    report: &mut impl Write,
) -> io::Result<()> {
    let text = retired.instruction.disassemble(retired.pc, style);
    let mut effects: Vec<String> = Vec::new();
    if let Some(write) = retired.register_write {
        effects.push(format!("{} = 0x{:08x}", write.reg.name(style), write.value));
    }
    if let Some(access) = retired.data_access {
        let verb = match access.kind {
            AccessKind::Store => "escribe",
            AccessKind::Load | AccessKind::Fetch => "lee",
        };
        effects.push(format!(
            "{verb} {} B en 0x{:08x}: 0x{:08x}",
            access.width.bytes(),
            access.address,
            access.value
        ));
    }
    let line = format!(
        "{cycle:>8}  {:08x}  {:08x}  {}  {text:<28} {}",
        retired.pc,
        retired.word,
        line_label(program, retired.pc),
        effects.join("; ")
    );
    writeln!(report, "{}", line.trim_end())
}

fn write_summary(
    program: &Program,
    cpu: &dyn Processor,
    ending: Ending,
    style: RegisterStyle,
    report: &mut impl Write,
) -> io::Result<()> {
    let output = cpu.state().console.output();
    // La salida del programa y el informe suelen acabar en la misma terminal: que el informe
    // empiece en su propia línea aunque el programa no imprimiera el salto final.
    if output.last().is_some_and(|&byte| byte != b'\n') {
        writeln!(report)?;
    }
    match ending {
        Ending::Halted(halt) => {
            match program.line_at(halt.pc) {
                Some(line) => writeln!(report, "parada en la línea {line}: {halt}")?,
                None => writeln!(report, "parada: {halt}")?,
            }
            if ran_off_the_end(program, halt) {
                writeln!(
                    report,
                    "el programa se salió del final del código: ¿falta la ecall de salida (a7 = 10)?"
                )?;
            }
        }
        Ending::CycleLimit => writeln!(
            report,
            "sin terminar tras {} ciclos (pc = 0x{:08x}); sube el tope con --max-cycles",
            cpu.stats().cycles,
            cpu.state().pc
        )?,
    }
    let stats = cpu.stats();
    writeln!(
        report,
        "ciclos: {}  instrucciones: {}  pc: 0x{:08x}",
        stats.cycles,
        stats.retired,
        cpu.state().pc
    )?;
    write_registers(cpu, style, report)
}

/// Olvidar la `ecall` de salida es el despiste más común: el `pc` llega justo al final del
/// texto y la búsqueda falla allí.
fn ran_off_the_end(program: &Program, halt: Halt) -> bool {
    u64::from(halt.pc) == program.text.end()
        && matches!(
            halt.trap,
            Trap::Memory(MemoryError::Unmapped {
                access: AccessKind::Fetch,
                ..
            })
        )
}

/// Los 32 registros en filas de cuatro.
fn write_registers(
    cpu: &dyn Processor,
    style: RegisterStyle,
    report: &mut impl Write,
) -> io::Result<()> {
    let registers = &cpu.state().registers;
    let cells: Vec<String> = (0u8..32)
        .filter_map(Reg::new)
        .map(|reg| format!("{:>4} 0x{:08x}", reg.name(style), registers.read(reg)))
        .collect();
    for row in cells.chunks(4) {
        writeln!(report, "{}", row.join("   "))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    struct Captured {
        ending: Ending,
        stats: Stats,
        out: String,
        report: String,
    }

    fn execute(source: &str, input: &str, options: RunOptions) -> Captured {
        let program = rv_asm::assemble(source).unwrap();
        let mut input = Cursor::new(input.as_bytes().to_vec());
        let mut out = Vec::new();
        let mut report = Vec::new();
        let (ending, stats) =
            run_program(&program, &options, &mut input, &mut out, &mut report).unwrap();
        Captured {
            ending,
            stats,
            out: String::from_utf8(out).unwrap(),
            report: String::from_utf8(report).unwrap(),
        }
    }

    const ECHO_DOUBLE: &str = ".data\nmsg: .string \"doble: \"\n.text\n\
        li a7, 5\n\
        ecall\n\
        add t0, a0, a0\n\
        la a0, msg\n\
        li a7, 4\n\
        ecall\n\
        mv a0, t0\n\
        li a7, 1\n\
        ecall\n\
        li a0, 3\n\
        li a7, 93\n\
        ecall\n";

    #[test]
    fn reads_stdin_and_reports_exit() {
        let run = execute(ECHO_DOUBLE, "21\n", RunOptions::default());
        assert_eq!(run.out, "doble: 42");
        assert_eq!(run.ending.exit_code(), 3);
        assert!(
            run.report.starts_with(
                "\nparada en la línea 15: 0x00000030: el programa terminó con código 3\n"
            ),
            "{}",
            run.report
        );
        assert!(run.report.contains("  a0 0x00000003"), "{}", run.report);
        assert!(run.report.contains("  t0 0x0000002a"), "{}", run.report);
    }

    #[test]
    fn closed_stdin_is_an_error_not_a_hang() {
        let run = execute(ECHO_DOUBLE, "", RunOptions::default());
        assert_eq!(
            run.ending,
            Ending::Halted(Halt {
                pc: 4,
                trap: Trap::EndOfInput
            })
        );
        assert_eq!(run.ending.exit_code(), 1);
        assert_eq!(run.stats.retired, 1);
        assert!(run.report.contains("línea 5"), "{}", run.report);
    }

    #[test]
    fn last_line_without_newline_still_counts() {
        let run = execute(ECHO_DOUBLE, "-5", RunOptions::default());
        assert_eq!(run.out, "doble: -10");
    }

    #[test]
    fn cycle_limit_stops_infinite_loops() {
        let options = RunOptions {
            max_cycles: 50,
            ..RunOptions::default()
        };
        let run = execute("loop: j loop\n", "", options);
        assert_eq!(run.ending, Ending::CycleLimit);
        assert_eq!(run.stats.cycles, 50);
        assert_eq!(run.ending.exit_code(), 1);
        assert!(run.report.starts_with("sin terminar tras 50 ciclos"));
    }

    #[test]
    fn running_off_the_end_suggests_the_exit_ecall() {
        let run = execute("li a0, 1\n", "", RunOptions::default());
        assert_eq!(run.ending.exit_code(), 1);
        assert!(
            run.report.contains("¿falta la ecall de salida"),
            "{}",
            run.report
        );
        let jumped = execute("j 0x40\n", "", RunOptions::default());
        assert!(!jumped.report.contains("¿falta la ecall de salida"));
    }

    #[test]
    fn breakpoint_ends_successfully() {
        let run = execute("li a0, 1\nebreak\n", "", RunOptions::default());
        assert_eq!(run.ending.exit_code(), 0);
        assert!(run.report.contains("punto de parada"));
    }

    #[test]
    fn faults_point_at_the_source_line() {
        let run = execute("nop\nsw a0, 0(zero)\n", "", RunOptions::default());
        assert_eq!(run.ending.exit_code(), 1);
        assert!(
            run.report.starts_with("parada en la línea 2: 0x00000004: "),
            "{}",
            run.report
        );
    }

    #[test]
    fn trace_lists_every_retired_instruction() {
        let options = RunOptions {
            trace: true,
            style: RegisterStyle::Numeric,
            ..RunOptions::default()
        };
        let run = execute(
            ".data\nv: .word 0\n.text\nla t0, v\nli t1, 7\nsw t1, 0(t0)\nebreak\n",
            "",
            options,
        );
        let trace: Vec<&str> = run.report.lines().take(4).collect();
        assert!(trace[0].starts_with("       1  00000000  "), "{trace:?}");
        assert!(trace[0].contains("L4"), "{trace:?}");
        assert!(trace[2].contains("x6 = 0x00000007"), "{trace:?}");
        assert!(
            trace[3].contains("escribe 4 B en 0x10000000: 0x00000007"),
            "{trace:?}"
        );
        assert!(run.report.contains("x31 0x00000000"));
    }

    #[test]
    fn exit_codes_keep_the_low_byte() {
        let exited = |code| {
            Ending::Halted(Halt {
                pc: 0,
                trap: Trap::Exited { code },
            })
        };
        assert_eq!(exited(0).exit_code(), 0);
        assert_eq!(exited(256 + 7).exit_code(), 7);
        assert_eq!(exited(-1).exit_code(), 255);
    }
}
