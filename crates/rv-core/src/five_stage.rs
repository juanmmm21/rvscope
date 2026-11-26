//! Procesador segmentado de cinco etapas (IF, ID, EX, MEM, WB) sin forwarding ni detección de
//! riesgos de datos: una instrucción que lee un registro que alguna de las dos anteriores aún no
//! ha escrito recibe el valor viejo. Los saltos se resuelven en EX y vacían lo que va detrás.
//! Las reglas de temporización y de paradas están en el ADR 0002.

use rv_asm::Program;
use rv_isa::{Instruction, Reg};

use crate::exec::{Operation, access_memory, execute};
use crate::map::MapError;
use crate::processor::{DataAccess, Halt, Processor, RegisterWrite, Retired, Stats, Trap};
use crate::state::ArchState;
use crate::syscall::{Console, EcallEffect, ecall};

/// Registro de segmentación entre dos etapas: vacío (burbuja), con una instrucción o con una
/// instrucción que ya ha fallado y viaja hasta WB para parar allí.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Latch<T> {
    Bubble,
    Holds(T),
    Fault(Halt),
}

/// IF/ID: la palabra buscada, sin decodificar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fetched {
    pc: u32,
    word: u32,
}

/// Instrucción decodificada que avanza por ID/EX, EX/MEM y MEM/WB. Cada etapa rellena lo suyo:
/// ID los operandos, EX `next_pc`, el resultado y la dirección, y MEM el valor cargado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Uop {
    pc: u32,
    word: u32,
    instruction: Instruction,
    operation: Operation,
    rs1: u32,
    rs2: u32,
    next_pc: u32,
    result: Option<u32>,
    address: Option<u32>,
    data_access: Option<DataAccess>,
}

impl Latch<Uop> {
    /// Lo que impide buscar instrucciones nuevas: una excepción o una `ecall` en vuelo.
    fn blocks_fetch(&self) -> bool {
        match self {
            Latch::Bubble => false,
            Latch::Fault(_) => true,
            Latch::Holds(uop) => uop.operation == Operation::Ecall,
        }
    }

    fn pc(&self) -> Option<u32> {
        match self {
            Latch::Bubble => None,
            Latch::Fault(halt) => Some(halt.pc),
            Latch::Holds(uop) => Some(uop.pc),
        }
    }
}

/// Resultado de la etapa WB cuando no falla.
enum WriteBack {
    Retired { retired: Retired, exit: Option<i32> },
    AwaitingInput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FiveStage {
    state: ArchState,
    stats: Stats,
    halt: Option<Halt>,
    awaiting_input: bool,
    /// `pc` de búsqueda; el arquitectónico es `state.pc`, que avanza al retirar.
    fetch_pc: u32,
    if_id: Latch<Fetched>,
    id_ex: Latch<Uop>,
    ex_mem: Latch<Uop>,
    mem_wb: Latch<Uop>,
    stage_pcs: [Option<u32>; 5],
}

impl FiveStage {
    pub fn new(program: &Program) -> Result<FiveStage, MapError> {
        Ok(FiveStage::from_state(ArchState::for_program(program)?))
    }

    /// Arranca con el pipeline vacío desde un estado ya preparado.
    pub fn from_state(state: ArchState) -> FiveStage {
        FiveStage {
            fetch_pc: state.pc,
            state,
            stats: Stats::default(),
            halt: None,
            awaiting_input: false,
            if_id: Latch::Bubble,
            id_ex: Latch::Bubble,
            ex_mem: Latch::Bubble,
            mem_wb: Latch::Bubble,
            stage_pcs: [None; 5],
        }
    }

    /// `pc` de la instrucción que ha pasado por cada etapa (IF, ID, EX, MEM, WB) en el último
    /// ciclo, incluidas las que un salto acaba de vaciar; `None` es una burbuja.
    pub fn stage_pcs(&self) -> [Option<u32>; 5] {
        self.stage_pcs
    }

    fn stop(&mut self, halt: Halt) {
        self.halt = Some(halt);
        self.if_id = Latch::Bubble;
        self.id_ex = Latch::Bubble;
        self.ex_mem = Latch::Bubble;
        self.mem_wb = Latch::Bubble;
    }

    /// WB: atiende la `ecall` si lo es, escribe `rd` y retira. Es el único punto donde cambian
    /// los registros y el `pc` arquitectónico.
    fn write_back(&mut self, uop: &Uop) -> Result<WriteBack, Trap> {
        let mut ecall_write = None;
        let mut exit = None;
        if uop.operation == Operation::Ecall {
            match ecall(&mut self.state)? {
                EcallEffect::Continue { register_write } => ecall_write = register_write,
                EcallEffect::Exit { code } => exit = Some(code),
                EcallEffect::AwaitingInput => return Ok(WriteBack::AwaitingInput),
            }
        }
        let register_write = uop
            .result
            .filter(|_| uop.instruction.rd != Reg::ZERO)
            .map(|value| RegisterWrite {
                reg: uop.instruction.rd,
                value,
            });
        if let Some(write) = register_write {
            self.state.registers.write(write.reg, write.value);
        }
        self.state.pc = uop.next_pc;
        Ok(WriteBack::Retired {
            retired: Retired {
                pc: uop.pc,
                word: uop.word,
                instruction: uop.instruction,
                next_pc: uop.next_pc,
                register_write: register_write.or(ecall_write),
                data_access: uop.data_access,
            },
            exit,
        })
    }

    /// MEM: carga o almacenamiento con la dirección calculada en EX.
    fn memory_stage(&mut self) -> Latch<Uop> {
        let Latch::Holds(mut uop) = self.ex_mem else {
            return self.ex_mem;
        };
        let Some(address) = uop.address else {
            return Latch::Holds(uop);
        };
        match access_memory(&mut self.state.memory, uop.operation, address, uop.rs2) {
            Ok((loaded, access)) => {
                uop.result = uop.result.or(loaded);
                uop.data_access = access;
                Latch::Holds(uop)
            }
            Err(trap) => Latch::Fault(Halt { pc: uop.pc, trap }),
        }
    }

    /// EX: la ALU y la decisión de salto. Devuelve también el destino si hay que vaciar.
    fn execute_stage(&self) -> (Latch<Uop>, Option<u32>) {
        let Latch::Holds(mut uop) = self.id_ex else {
            return (self.id_ex, None);
        };
        match execute(uop.operation, &uop.instruction, uop.pc, uop.rs1, uop.rs2) {
            Ok(execution) => {
                uop.next_pc = execution.next_pc;
                uop.result = execution.result;
                uop.address = execution.address;
                let redirect =
                    (execution.next_pc != uop.pc.wrapping_add(4)).then_some(execution.next_pc);
                (Latch::Holds(uop), redirect)
            }
            Err(trap) => (Latch::Fault(Halt { pc: uop.pc, trap }), None),
        }
    }

    /// ID: decodifica y lee los registros. Como WB ya ha escrito en este ciclo, se ve lo que
    /// retira la instrucción de tres por delante, pero no lo de las dos anteriores.
    fn decode_stage(&self) -> Latch<Uop> {
        let fetched = match self.if_id {
            Latch::Holds(fetched) => fetched,
            Latch::Bubble => return Latch::Bubble,
            Latch::Fault(halt) => return Latch::Fault(halt),
        };
        let fault = |trap| {
            Latch::Fault(Halt {
                pc: fetched.pc,
                trap,
            })
        };
        let instruction = match Instruction::decode(fetched.word) {
            Ok(instruction) => instruction,
            Err(error) => return fault(Trap::Decode(error)),
        };
        let operation = Operation::of(instruction.mnemonic);
        if operation == Operation::Ebreak {
            return fault(Trap::Breakpoint);
        }
        Latch::Holds(Uop {
            pc: fetched.pc,
            word: fetched.word,
            instruction,
            operation,
            rs1: self.state.registers.read(instruction.rs1),
            rs2: self.state.registers.read(instruction.rs2),
            next_pc: fetched.pc.wrapping_add(4),
            result: None,
            address: None,
            data_access: None,
        })
    }

    /// IF: busca en `fetch_pc` y avanza. Un fallo de búsqueda puede ser de un camino que un
    /// salto anterior aún va a vaciar, así que no para nada hasta llegar a WB.
    fn fetch_stage(&mut self) -> Latch<Fetched> {
        let pc = self.fetch_pc;
        match self.state.memory.fetch(pc) {
            Ok(word) => {
                self.fetch_pc = pc.wrapping_add(4);
                Latch::Holds(Fetched { pc, word })
            }
            Err(error) => Latch::Fault(Halt {
                pc,
                trap: Trap::Memory(error),
            }),
        }
    }
}

impl Processor for FiveStage {
    fn name(&self) -> &'static str {
        "5 etapas sin forwarding ni detección de riesgos"
    }

    fn state(&self) -> &ArchState {
        &self.state
    }

    fn stats(&self) -> Stats {
        self.stats
    }

    fn halt(&self) -> Option<Halt> {
        self.halt
    }

    fn console_mut(&mut self) -> &mut Console {
        &mut self.state.console
    }

    fn awaiting_input(&self) -> bool {
        self.awaiting_input
    }

    /// Las etapas se evalúan de WB hacia IF: así WB escribe antes de que ID lea, y una parada
    /// en WB impide que lo que va detrás toque la memoria.
    fn step(&mut self) -> Vec<Retired> {
        if self.halt.is_some() {
            return Vec::new();
        }
        let older_pcs = [
            match self.if_id {
                Latch::Bubble => None,
                Latch::Holds(fetched) => Some(fetched.pc),
                Latch::Fault(halt) => Some(halt.pc),
            },
            self.id_ex.pc(),
            self.ex_mem.pc(),
            self.mem_wb.pc(),
        ];

        let mut retired = Vec::new();
        let mut exit = None;
        match self.mem_wb {
            Latch::Bubble => {}
            Latch::Fault(halt) => {
                self.stop(halt);
                return Vec::new();
            }
            Latch::Holds(uop) => match self.write_back(&uop) {
                Ok(WriteBack::Retired {
                    retired: entry,
                    exit: code,
                }) => {
                    retired.push(entry);
                    exit = code.map(|code| (uop.pc, code));
                }
                Ok(WriteBack::AwaitingInput) => {
                    self.awaiting_input = true;
                    return Vec::new();
                }
                Err(trap) => {
                    self.stop(Halt { pc: uop.pc, trap });
                    return Vec::new();
                }
            },
        }
        self.awaiting_input = false;
        self.stats.cycles += 1;
        self.stats.retired += retired.len() as u64;
        if let Some((pc, code)) = exit {
            self.stage_pcs = [None, None, None, None, older_pcs[3]];
            self.stop(Halt {
                pc,
                trap: Trap::Exited { code },
            });
            return retired;
        }

        let mem_wb = self.memory_stage();
        let (ex_mem, redirect) = self.execute_stage();
        // En un ciclo de vaciado IF e ID también trabajan (en el camino equivocado) y se ve,
        // aunque lo que hacen se descarta.
        let fetching_pc = self.fetch_pc;
        let (id_ex, if_id, fetched) = match redirect {
            Some(target) => {
                self.fetch_pc = target;
                (Latch::Bubble, Latch::Bubble, !mem_wb.blocks_fetch())
            }
            None => {
                let id_ex = self.decode_stage();
                let blocked =
                    id_ex.blocks_fetch() || ex_mem.blocks_fetch() || mem_wb.blocks_fetch();
                let if_id = if blocked {
                    Latch::Bubble
                } else {
                    self.fetch_stage()
                };
                (id_ex, if_id, !blocked)
            }
        };
        self.stage_pcs = [
            fetched.then_some(fetching_pc),
            older_pcs[0],
            older_pcs[1],
            older_pcs[2],
            older_pcs[3],
        ];
        self.mem_wb = mem_wb;
        self.ex_mem = ex_mem;
        self.id_ex = id_ex;
        self.if_id = if_id;
        retired
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::SegmentKind;
    use crate::memory::{AccessKind, MemoryError, Width};
    use crate::processor::RunOutcome;
    use crate::single_cycle::SingleCycle;
    use rv_isa::DecodeError;

    fn program(source: &str) -> Program {
        rv_asm::assemble(source).unwrap()
    }

    fn boot(source: &str) -> FiveStage {
        FiveStage::new(&program(source)).unwrap()
    }

    fn read(cpu: &impl Processor, name: &str) -> u32 {
        cpu.state().registers.read(Reg::parse(name).unwrap())
    }

    /// Ejecuta hasta pararse y devuelve todo lo retirado por orden.
    fn trace(cpu: &mut impl Processor) -> Vec<Retired> {
        let mut retired = Vec::new();
        for _ in 0..10_000 {
            if cpu.halt().is_some() {
                break;
            }
            retired.extend(cpu.step());
        }
        assert!(cpu.halt().is_some(), "no se ha parado");
        retired
    }

    /// Un programa sin riesgos de datos retira lo mismo, en el mismo orden, y acaba igual que
    /// en el de un ciclo.
    fn assert_matches_single_cycle(source: &str) -> FiveStage {
        let program = program(source);
        let mut reference = SingleCycle::new(&program).unwrap();
        let mut pipelined = FiveStage::new(&program).unwrap();
        assert_eq!(trace(&mut pipelined), trace(&mut reference));
        assert_eq!(pipelined.state(), reference.state());
        assert_eq!(pipelined.halt(), reference.halt());
        assert_eq!(pipelined.stats().retired, reference.stats().retired);
        pipelined
    }

    /// Suma de 1 a 10, copia de un vector a la pila y una llamada, con dos instrucciones
    /// independientes o `nop` entre cada productor y su consumidor. Sin `la` ni `call`: se
    /// expanden en dos instrucciones dependientes entre sí.
    const HAZARD_FREE: &str = ".data\nsrc: .word 3, 5, 7\n.text\n\
         li t0, 10\nli a0, 0\nnop\nnop\n\
         loop: add a0, a0, t0\n\
         addi t0, t0, -1\n\
         nop\nnop\n\
         bnez t0, loop\n\
         lui t1, %hi(src)\nli t2, 3\nnop\n\
         addi t1, t1, %lo(src)\nnop\nnop\n\
         copy: addi sp, sp, -4\n\
         lw t3, 0(t1)\n\
         addi t1, t1, 4\n\
         addi t2, t2, -1\n\
         sw t3, 0(sp)\n\
         nop\n\
         bnez t2, copy\n\
         lw a1, 0(sp)\nlw a2, 8(sp)\n\
         jal ra, f\n\
         nop\nnop\n\
         add a4, a3, a3\n\
         li a7, 93\nnop\nnop\n\
         ecall\n\
         f: li a3, 21\n\
         ret\n";

    #[test]
    fn hazard_free_programs_match_the_single_cycle_model() {
        let cpu = assert_matches_single_cycle(HAZARD_FREE);
        assert_eq!(read(&cpu, "a0"), 55);
        assert_eq!((read(&cpu, "a1"), read(&cpu, "a2")), (7, 3));
        assert_eq!(read(&cpu, "a4"), 42);
        assert_eq!(
            cpu.halt().map(|halt| halt.trap),
            Some(Trap::Exited { code: 55 })
        );
    }

    #[test]
    fn independent_instructions_retire_one_per_cycle_after_filling() {
        let mut cpu = boot("li a0, 1\nli a1, 2\nli a2, 3\nli a3, 4\nebreak\n");
        for _ in 0..4 {
            assert!(cpu.step().is_empty());
        }
        assert_eq!(cpu.stage_pcs(), [Some(12), Some(8), Some(4), Some(0), None]);
        assert_eq!(cpu.step()[0].pc, 0);
        assert_eq!(
            cpu.stage_pcs(),
            [Some(16), Some(12), Some(8), Some(4), Some(0)]
        );
        assert_eq!(
            cpu.run(100),
            RunOutcome::Halted(Halt {
                pc: 16,
                trap: Trap::Breakpoint
            })
        );
        assert_eq!(
            cpu.stats(),
            Stats {
                cycles: 8,
                retired: 4
            }
        );
        assert_eq!(cpu.state().pc, 16);
    }

    #[test]
    fn dependent_instructions_read_stale_values() {
        let mut cpu = boot("li a0, 5\naddi a1, a0, 1\naddi a2, a0, 1\naddi a3, a0, 1\nebreak\n");
        cpu.run(100);
        // Las dos siguientes leen a0 antes de que se escriba; la tercera ya lo ve.
        assert_eq!(
            (read(&cpu, "a1"), read(&cpu, "a2"), read(&cpu, "a3")),
            (1, 1, 6)
        );
    }

    #[test]
    fn loaded_values_arrive_three_instructions_later() {
        let mut cpu = boot(
            ".data\nv: .word 9\n.text\nlui t0, %hi(v)\nnop\nnop\naddi t0, t0, %lo(v)\nnop\nnop\n\
             lw a0, 0(t0)\nmv a1, a0\nmv a2, a0\nmv a3, a0\nebreak\n",
        );
        cpu.run(100);
        assert_eq!(
            (read(&cpu, "a1"), read(&cpu, "a2"), read(&cpu, "a3")),
            (0, 0, 9)
        );
    }

    #[test]
    fn stores_use_the_value_read_in_decode() {
        let mut cpu = boot("li t0, 7\nsw t0, -4(sp)\nnop\nnop\nlw a0, -4(sp)\nnop\nnop\nebreak\n");
        cpu.run(100);
        assert_eq!(read(&cpu, "a0"), 0);
    }

    #[test]
    fn taken_branches_flush_the_two_younger_instructions() {
        let mut cpu = boot("li t0, 1\nnop\nnop\nbnez t0, skip\nli a0, 1\nli a1, 1\nskip: ebreak\n");
        let retired = trace(&mut cpu);
        assert_eq!((read(&cpu, "a0"), read(&cpu, "a1")), (0, 0));
        assert_eq!(retired.len(), 4);
        // 4 retiradas + 4 de llenado + 2 de penalización por el salto tomado.
        assert_eq!(cpu.stats().cycles, 10);

        // En el ciclo en que el salto está en EX, ID e IF tienen las dos que se vacían.
        let mut cpu = boot("li t0, 1\nnop\nnop\nbnez t0, skip\nli a0, 1\nli a1, 1\nskip: ebreak\n");
        for _ in 0..6 {
            cpu.step();
        }
        assert_eq!(
            cpu.stage_pcs(),
            [Some(20), Some(16), Some(12), Some(8), Some(4)]
        );
        cpu.step();
        assert_eq!(cpu.stage_pcs(), [Some(24), None, None, Some(12), Some(8)]);
    }

    #[test]
    fn branches_decide_with_stale_operands() {
        let mut cpu = boot("li t0, 1\nbnez t0, skip\nli a0, 1\nskip: ebreak\n");
        cpu.run(100);
        assert_eq!(read(&cpu, "a0"), 1);
    }

    #[test]
    fn faults_on_the_wrong_path_are_flushed() {
        let mut cpu = boot("j end\n.word 0xffffffff\n.word 0xffffffff\nend: ebreak\n");
        assert_eq!(
            cpu.run(100),
            RunOutcome::Halted(Halt {
                pc: 12,
                trap: Trap::Breakpoint
            })
        );
    }

    #[test]
    fn traps_are_precise() {
        let source = "li a0, 1\nnop\nnop\nlw a0, 2(sp)\nli a1, 7\nsw a1, -4(sp)\nnop\n";
        let cpu = assert_matches_single_cycle(source);
        assert_eq!(
            cpu.halt(),
            Some(Halt {
                pc: 12,
                trap: Trap::Memory(MemoryError::Misaligned {
                    address: 0x7fff_fff2,
                    width: Width::Word,
                    access: AccessKind::Load
                })
            })
        );
        assert_eq!(read(&cpu, "a1"), 0);
        assert_eq!(cpu.state().pc, 12);

        assert_matches_single_cycle(".word 0xffffffff\n");
        assert_matches_single_cycle("nop\n");
        assert_matches_single_cycle("li t0, 6\nnop\nnop\njalr ra, 0(t0)\nnop\nnop\n");
        let mut cpu = boot("sw zero, 0(zero)\n");
        cpu.run(100);
        assert_eq!(
            cpu.halt().map(|halt| halt.trap),
            Some(Trap::Memory(MemoryError::PermissionDenied {
                address: 0,
                access: AccessKind::Store,
                segment: SegmentKind::Text
            }))
        );
        let mut cpu = boot(".word 0xffffffff\n");
        cpu.run(100);
        assert_eq!(
            cpu.halt().map(|halt| halt.trap),
            Some(Trap::Decode(DecodeError::Illegal(0xffff_ffff)))
        );
    }

    const ECHO: &str = ".data\nprompt: .string \"n? \"\n.text\n\
         lui a0, %hi(prompt)\nnop\nnop\naddi a0, a0, %lo(prompt)\nli a7, 4\necall\n\
         li a7, 5\necall\n\
         slli a0, a0, 1\nli a7, 1\necall\n\
         li a0, 10\nli a7, 11\necall\n\
         li a7, 10\necall\n";

    #[test]
    fn ecalls_see_every_older_write_and_wait_for_input() {
        let mut cpu = boot(ECHO);
        assert_eq!(cpu.run(1_000), RunOutcome::AwaitingInput);
        let waiting = cpu.clone();
        assert!(cpu.step().is_empty());
        assert_eq!(cpu, waiting);
        cpu.console_mut().push_input("21\n");
        assert!(matches!(cpu.run(1_000), RunOutcome::Halted(_)));
        assert_eq!(cpu.state().console.output_text(), "n? 42\n");

        let mut eager = boot(ECHO);
        eager.console_mut().push_input("21\n");
        eager.run(1_000);
        assert_eq!(eager.state(), cpu.state());
        assert_eq!(eager.stats(), cpu.stats());

        let mut reference = SingleCycle::new(&program(ECHO)).unwrap();
        reference.console_mut().push_input("21\n");
        reference.run(1_000);
        assert_eq!(reference.state(), cpu.state());
    }

    #[test]
    fn exit_retires_and_stops_without_fetching_past_it() {
        let mut cpu = boot("li a0, 3\nli a7, 93\nnop\nnop\necall\n");
        assert_eq!(
            cpu.run(100),
            RunOutcome::Halted(Halt {
                pc: 16,
                trap: Trap::Exited { code: 3 }
            })
        );
        assert_eq!(cpu.state().pc, 20);
        assert_eq!(cpu.stats().retired, 5);
    }

    #[test]
    fn programs_with_data_hazards_go_wrong() {
        // `la` y `call` son dos instrucciones dependientes: el factorial no llega a 720.
        let source = include_str!("../../../examples/factorial.s");
        let mut reference = SingleCycle::new(&program(source)).unwrap();
        reference.run(10_000);
        let mut cpu = boot(source);
        cpu.run(10_000);
        assert_ne!(cpu.state(), reference.state());
        assert_ne!(read(&cpu, "a0"), 720);
    }

    #[test]
    fn execution_is_deterministic() {
        let mut first = boot(HAZARD_FREE);
        let mut second = boot(HAZARD_FREE);
        assert_eq!(trace(&mut first), trace(&mut second));
        assert_eq!(first, second);
    }
}
