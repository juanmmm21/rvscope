//! Todos los modelos de procesador tienen que ser indistinguibles del de un ciclo desde fuera:
//! las mismas instrucciones retiradas en el mismo orden y con los mismos efectos, la misma
//! parada (dónde y por qué), lo mismo impreso y el mismo estado final. Los riscv-tests solo
//! terminan con la `ecall` de salida; aquí se cubren las otras formas de acabar: cada
//! excepción, fallos en la sombra de un salto o detrás de la salida (que no deben ocurrir),
//! fallos en cualquiera de las dos vías de la doble emisión y programas que leen entrada.

#![allow(clippy::unwrap_used)]

use rv_asm::{Program, assemble};
use rv_core::{
    ArchState, DualIssue, FiveStage, Halt, Hazards, Processor, Retired, SingleCycle, Stats, Trap,
};

type Factory = fn(&Program) -> Box<dyn Processor>;

const MODELS: [(&str, Factory); 4] = [
    ("un ciclo", |program| {
        Box::new(SingleCycle::new(program).unwrap())
    }),
    ("5 etapas con burbujas", |program| {
        Box::new(FiveStage::new(program, Hazards::Stall).unwrap())
    }),
    ("5 etapas con forwarding", |program| {
        Box::new(FiveStage::new(program, Hazards::Forward).unwrap())
    }),
    ("doble emisión", |program| {
        Box::new(DualIssue::new(program).unwrap())
    }),
];

/// Entrada que se da al programa: cada trozo se entrega cuando una `ecall` lo espera y, al
/// acabarse, la consola se cierra.
type Input = &'static [&'static str];

/// Lo observable de una ejecución completa.
#[derive(Debug)]
struct Outcome {
    retired: Vec<Retired>,
    halt: Halt,
    state: ArchState,
    output: String,
}

fn execute(program: &Program, factory: Factory, input: Input) -> (Outcome, Stats) {
    let mut cpu = factory(program);
    let mut pending = input.iter();
    let mut retired = Vec::new();
    for _ in 0..100_000 {
        if let Some(halt) = cpu.halt() {
            assert!(
                !cpu.awaiting_input(),
                "{}: parado y esperando entrada",
                cpu.name()
            );
            let state = cpu.state().clone();
            let output = state.console.output_text();
            let outcome = Outcome {
                retired,
                halt,
                state,
                output,
            };
            return (outcome, cpu.stats());
        }
        retired.extend(cpu.step());
        if cpu.awaiting_input() {
            match pending.next() {
                Some(chunk) => cpu.console_mut().push_input(chunk),
                None => cpu.console_mut().close_input(),
            }
        }
    }
    panic!("{} no se ha parado", cpu.name());
}

/// Ejecuta en todos los modelos, exige que coincidan con el de un ciclo y devuelve su parada.
fn equivalent(source: &str, input: Input) -> Halt {
    let program = assemble(source).unwrap_or_else(|errors| panic!("{source}\n{errors:?}"));
    let (reference, reference_stats) = execute(&program, MODELS[0].1, input);
    for (name, factory) in &MODELS[1..] {
        let (outcome, stats) = execute(&program, *factory, input);
        let context = format!("{name} no acaba igual que el de un ciclo:\n{source}");
        assert_eq!(outcome.retired, reference.retired, "{context}");
        assert_eq!(outcome.halt, reference.halt, "{context}");
        assert_eq!(outcome.output, reference.output, "{context}");
        assert_eq!(outcome.state.pc, reference.state.pc, "{context}");
        assert_eq!(
            outcome.state.registers, reference.state.registers,
            "{context}"
        );
        // Sin `assert_eq!`: el volcado de la memoria entera no ayuda a ver la diferencia.
        assert!(
            outcome.state.memory == reference.state.memory,
            "memoria. {context}"
        );
        assert!(
            outcome.state.console == reference.state.console,
            "consola. {context}"
        );
        assert_eq!(stats.retired, reference_stats.retired, "{name}");
        assert!(stats.cycles >= reference_stats.cycles, "{name}");
    }
    reference.halt
}

fn trap(source: &str) -> Trap {
    equivalent(source, &[]).trap
}

/// Antepone `n` instrucciones independientes para que lo que sigue caiga en cada vía y cada
/// alineamiento de la ventana de búsqueda de la doble emisión.
fn shifted(n: usize, body: &str) -> String {
    let padding = "addi s1, s1, 1\n".repeat(n);
    format!("{padding}{body}")
}

#[test]
fn memory_exceptions_stop_every_model_at_the_same_place() {
    let cases = [
        // Carga desalineada con la dirección recién calculada (adelantada).
        "li t0, 0x10000001\nli a0, 7\nlw a0, 0(t0)\nli a7, 10\necall\n",
        // Escribir en el código.
        "la t0, here\nhere: li a0, 3\nsw a0, 0(t0)\nli a7, 10\necall\n",
        // Fuera del mapa, con un almacenamiento justo antes que no debe perderse.
        ".data\nx: .word 0\n.text\nla t1, x\nli a0, 9\nsw a0, 0(t1)\n\
         li t0, 0x50000000\nlw a1, 0(t0)\nli a7, 10\necall\n",
        // Media palabra desalineada en un almacenamiento.
        "li t0, 0x10000003\nsh t0, 0(t0)\nli a7, 10\necall\n",
    ];
    for body in cases {
        for n in 0..4 {
            assert!(matches!(trap(&shifted(n, body)), Trap::Memory(_)), "{body}");
        }
    }
}

#[test]
fn control_flow_exceptions_stop_every_model_at_the_same_place() {
    for n in 0..4 {
        // Palabra ilegal alcanzada de verdad.
        let illegal = shifted(n, "li a0, 1\n.word 0xffffffff\nli a7, 10\necall\n");
        assert!(matches!(trap(&illegal), Trap::Decode(_)));
        // Destino de `jalr` no alineado a 4, calculado justo antes: no escribe `ra`.
        let jalr = shifted(n, "li ra, 0\nli t0, 6\njalr ra, t0, 0\nli a7, 10\necall\n");
        assert_eq!(trap(&jalr), Trap::MisalignedTarget { target: 6 });
        let ebreak = shifted(n, "li a0, 5\nebreak\nli a0, 6\nli a7, 10\necall\n");
        assert_eq!(trap(&ebreak), Trap::Breakpoint);
        // Sin `ecall` de salida: el `pc` se sale del código.
        let falls_off = shifted(n, "li a0, 5\naddi a0, a0, 1\n");
        assert!(matches!(trap(&falls_off), Trap::Memory(_)));
        let unknown = shifted(n, "li a7, 999\necall\nli a7, 10\necall\n");
        assert_eq!(trap(&unknown), Trap::UnknownSyscall { number: 999 });
    }
}

#[test]
fn wrong_path_and_post_exit_faults_never_happen() {
    for n in 0..4 {
        // Lo buscado tras un salto tomado se descarta sin fallar.
        let shadow = shifted(
            n,
            "li a0, 1\nbnez a0, ok\n.word 0xffffffff\nlw a1, 1(zero)\n\
             ok: j end\n.word 0\n.word 0\nend: li a7, 10\necall\n",
        );
        assert_eq!(trap(&shadow), Trap::Exited { code: 0 });
        // Tras la `ecall` de salida no se ejecuta nada, aunque lo de detrás falle.
        let after_exit = shifted(n, "li a0, 4\nli a7, 93\necall\nlw a1, 1(zero)\n.word 0\n");
        assert_eq!(trap(&after_exit), Trap::Exited { code: 4 });
    }
}

#[test]
fn a_fault_in_either_lane_keeps_only_what_is_older() {
    for n in 0..4 {
        // La vieja falla: la joven (independiente) no debe escribir `a2`.
        let older = shifted(
            n,
            "li a2, 1\nlw a1, 1(zero)\naddi a2, zero, 5\nli a7, 10\necall\n",
        );
        // La joven falla: la vieja (independiente) sí se retira.
        let younger = shifted(
            n,
            "li a2, 1\naddi a2, zero, 5\nlw a1, 1(zero)\nli a7, 10\necall\n",
        );
        for source in [older, younger] {
            assert!(matches!(trap(&source), Trap::Memory(_)));
        }
    }
}

const ECHO_SUM: &str = "li a7, 5\necall\nmv t0, a0\nli a7, 5\necall\nadd a0, a0, t0\n\
     li a7, 1\necall\nli a0, 10\nli a7, 11\necall\nli a7, 10\necall\n";

#[test]
fn programs_reading_input_behave_the_same() {
    assert_eq!(
        equivalent(ECHO_SUM, &["40\n", "2\n"]).trap,
        Trap::Exited { code: 0 }
    );
    // Una línea partida en dos entregas espera a estar completa.
    assert_eq!(
        equivalent(ECHO_SUM, &["4", "0\n2\n"]).trap,
        Trap::Exited { code: 0 }
    );
    assert_eq!(equivalent(ECHO_SUM, &["40\n"]).trap, Trap::EndOfInput);
    assert_eq!(equivalent(ECHO_SUM, &["hola\n"]).trap, Trap::InvalidInteger);

    // Leer una cadena al búfer, un carácter y volver a imprimirlos.
    let echo_text = ".data\nbuf: .space 16\n.text\n\
         la a0, buf\nli a1, 16\nli a7, 8\necall\n\
         li a7, 12\necall\nmv s0, a0\n\
         la a0, buf\nli a7, 4\necall\n\
         mv a0, s0\nli a7, 11\necall\n\
         lbu a0, buf\nli a7, 34\necall\nli a7, 10\necall\n";
    assert_eq!(
        equivalent(echo_text, &["rvscope\n", "x"]).trap,
        Trap::Exited { code: 0 }
    );
}

#[test]
fn every_print_service_prints_the_same() {
    let source = ".data\ns: .string \"fin\"\n.text\n\
         li a0, -7\nli a7, 1\necall\n\
         li a7, 34\necall\nli a7, 35\necall\nli a7, 36\necall\n\
         li a0, 33\nli a7, 11\necall\n\
         la a0, s\nli a7, 4\necall\n\
         li a0, 3\nli a7, 17\necall\n";
    assert_eq!(equivalent(source, &[]).trap, Trap::Exited { code: 3 });
}

#[test]
fn loops_calls_and_the_stack_end_the_same() {
    let factorial = include_str!("../../../examples/factorial.s");
    assert_eq!(equivalent(factorial, &[]).trap, Trap::Exited { code: 0 });

    // Fibonacci recursivo: muchas llamadas, la pila, carga-uso y saltos en las dos direcciones.
    let fibonacci = "li a0, 10\ncall fib\nli a7, 1\necall\nli a7, 10\necall\n\
         fib: li t0, 2\nblt a0, t0, small\n\
         addi sp, sp, -12\nsw ra, 8(sp)\nsw a0, 4(sp)\n\
         addi a0, a0, -1\ncall fib\nsw a0, 0(sp)\n\
         lw a0, 4(sp)\naddi a0, a0, -2\ncall fib\n\
         lw t1, 0(sp)\nadd a0, a0, t1\n\
         lw ra, 8(sp)\naddi sp, sp, 12\n\
         small: ret\n";
    assert_eq!(equivalent(fibonacci, &[]).trap, Trap::Exited { code: 0 });

    // Burbuja sobre un vector: cargas, almacenamientos y comparaciones dependientes.
    let sort = ".data\nv: .word 9, -3, 27, 0, 14, -8, 5, 1\n.text\n\
         li s0, 8\nouter: addi s0, s0, -1\nblez s0, done\n\
         la t0, v\nmv t1, s0\n\
         inner: lw t2, 0(t0)\nlw t3, 4(t0)\nble t2, t3, keep\n\
         sw t3, 0(t0)\nsw t2, 4(t0)\n\
         keep: addi t0, t0, 4\naddi t1, t1, -1\nbnez t1, inner\nj outer\n\
         done: lw a0, v\nli a7, 1\necall\nli a7, 10\necall\n";
    assert_eq!(equivalent(sort, &[]).trap, Trap::Exited { code: 0 });
}
