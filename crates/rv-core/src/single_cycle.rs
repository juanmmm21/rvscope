//! Procesador de un ciclo: cada ciclo lee, decodifica, ejecuta y retira una instrucción entera.
//! Es la referencia arquitectónica con la que se comparan los modelos segmentados.

use rv_asm::Program;
use rv_isa::{Instruction, Reg};

use crate::exec::{Operation, access_memory, execute};
use crate::map::MapError;
use crate::processor::{Halt, Processor, RegisterWrite, Retired, Stats, Trap};
use crate::state::ArchState;
use crate::syscall::{Console, EcallEffect, ecall};
use crate::view::{PipelineView, Progress, Slot, SlotState, Stage};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SingleCycle {
    state: ArchState,
    stats: Stats,
    view: PipelineView,
    halt: Option<Halt>,
    awaiting_input: bool,
}

/// Lo que deja un ciclo que no ha fallado.
enum Executed {
    Retired {
        retired: Retired,
        progress: Progress,
        exit: Option<i32>,
    },
    AwaitingInput,
}

impl SingleCycle {
    pub fn new(program: &Program) -> Result<SingleCycle, MapError> {
        Ok(SingleCycle::from_state(ArchState::for_program(program)?))
    }

    /// Arranca desde un estado ya preparado (la CLI y los tests de conformidad lo usan).
    pub fn from_state(state: ArchState) -> SingleCycle {
        SingleCycle {
            state,
            stats: Stats::default(),
            view: PipelineView::empty(&[Stage::Single], 1),
            halt: None,
            awaiting_input: false,
        }
    }

    fn record(&mut self, slot: Slot) {
        self.view = PipelineView::empty(&[Stage::Single], 1);
        self.view.cycle = self.stats.cycles;
        self.view.stages[0].lanes[0] = Some(slot);
    }

    /// Ejecuta la instrucción de `pc`. Si algo falla, el estado no se toca: primero se calcula
    /// todo y solo al final se escriben memoria, registro y `pc`.
    fn execute(&mut self) -> Result<Executed, Trap> {
        let state = &mut self.state;
        let pc = state.pc;
        let word = state.memory.fetch(pc)?;
        let instruction = Instruction::decode(word)?;
        let operation = Operation::of(instruction.mnemonic);
        if operation == Operation::Ebreak {
            return Err(Trap::Breakpoint);
        }
        let rs1 = state.registers.read(instruction.rs1);
        let rs2 = state.registers.read(instruction.rs2);
        let execution = execute(operation, &instruction, pc, rs1, rs2)?;

        let mut result = execution.result;
        let mut data_access = None;
        let mut ecall_write: Option<RegisterWrite> = None;
        let mut exit: Option<i32> = None;
        if let Some(address) = execution.address {
            let (loaded, access) = access_memory(&mut state.memory, operation, address, rs2)?;
            result = result.or(loaded);
            data_access = access;
        }
        if operation == Operation::Ecall {
            match ecall(state)? {
                EcallEffect::Continue { register_write } => ecall_write = register_write,
                EcallEffect::Exit { code } => exit = Some(code),
                EcallEffect::AwaitingInput => return Ok(Executed::AwaitingInput),
            }
        }

        let register_write =
            result
                .filter(|_| instruction.rd != Reg::ZERO)
                .map(|value| RegisterWrite {
                    reg: instruction.rd,
                    value,
                });
        if let Some(write) = register_write {
            state.registers.write(write.reg, write.value);
        }
        state.pc = execution.next_pc;
        Ok(Executed::Retired {
            progress: Progress {
                pc,
                word,
                instruction,
                operands: [rs1, rs2],
                next_pc: execution.next_pc,
                result,
                address: execution.address,
                data_access,
            },
            retired: Retired {
                pc,
                word,
                instruction,
                next_pc: execution.next_pc,
                register_write: register_write.or(ecall_write),
                data_access,
            },
            exit,
        })
    }
}

impl Processor for SingleCycle {
    fn name(&self) -> &'static str {
        "Un ciclo"
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

    fn view(&self) -> &PipelineView {
        &self.view
    }

    fn console_mut(&mut self) -> &mut Console {
        &mut self.state.console
    }

    fn awaiting_input(&self) -> bool {
        self.awaiting_input
    }

    fn step(&mut self) -> Vec<Retired> {
        if self.halt.is_some() {
            return Vec::new();
        }
        let pc = self.state.pc;
        let executed = self.execute();
        self.awaiting_input = matches!(executed, Ok(Executed::AwaitingInput));
        match executed {
            Ok(Executed::Retired {
                retired,
                progress,
                exit,
            }) => {
                self.stats.cycles += 1;
                self.stats.retired += 1;
                let mut slot = Slot::decoded(Stage::Single, SlotState::Active, progress, [None; 2]);
                slot.retire(&retired);
                if let Some(code) = exit {
                    slot.trap = Some(Trap::Exited { code });
                }
                self.record(slot);
                if let Some(code) = exit {
                    self.halt = Some(Halt {
                        pc,
                        trap: Trap::Exited { code },
                    });
                }
                vec![retired]
            }
            Ok(Executed::AwaitingInput) => Vec::new(),
            Err(trap) => {
                // La instrucción que falla no deja nada calculado: se enseña con lo que se
                // pueda buscar y decodificar de ella.
                let mut slot = match self.state.memory.fetch(pc) {
                    Ok(word) => Slot::fetched(pc, word, SlotState::Active),
                    Err(_) => Slot::fault(pc, trap, SlotState::Active),
                };
                slot.trap = Some(trap);
                self.record(slot);
                self.halt = Some(Halt { pc, trap });
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{AccessKind, MemoryError, Width};
    use crate::processor::DataAccess;
    use crate::processor::RunOutcome;
    use rv_isa::DecodeError;

    fn boot(source: &str) -> SingleCycle {
        SingleCycle::new(&rv_asm::assemble(source).unwrap()).unwrap()
    }

    fn reg(name: &str) -> Reg {
        Reg::parse(name).unwrap()
    }

    fn read(cpu: &SingleCycle, name: &str) -> u32 {
        cpu.state().registers.read(reg(name))
    }

    /// Ejecuta hasta el primer `ebreak`, que marca el final en estos tests.
    fn run(source: &str) -> SingleCycle {
        let mut cpu = boot(source);
        let outcome = cpu.run(10_000);
        assert!(
            matches!(
                outcome,
                RunOutcome::Halted(Halt {
                    trap: Trap::Breakpoint,
                    ..
                })
            ),
            "{outcome:?}"
        );
        cpu
    }

    #[test]
    fn arithmetic_and_immediates() {
        let cpu = run("li a0, 0x12345678\n\
             addi a1, a0, -0x678\n\
             sub a2, zero, a0\n\
             srai a3, a2, 4\n\
             srli a4, a2, 28\n\
             slti a5, a2, 0\n\
             sltiu a6, a2, 1\n\
             lui t0, 0xfffff\n\
             xori t1, t0, -1\n\
             ebreak\n");
        assert_eq!(read(&cpu, "a0"), 0x1234_5678);
        assert_eq!(read(&cpu, "a1"), 0x1234_5000);
        assert_eq!(read(&cpu, "a2"), 0xedcb_a988);
        assert_eq!(read(&cpu, "a3"), 0xfedc_ba98);
        assert_eq!(read(&cpu, "a4"), 0xe);
        assert_eq!(read(&cpu, "a5"), 1);
        assert_eq!(read(&cpu, "a6"), 0);
        assert_eq!(read(&cpu, "t0"), 0xffff_f000);
        assert_eq!(read(&cpu, "t1"), 0x0000_0fff);
    }

    #[test]
    fn writes_to_zero_are_discarded() {
        let mut cpu = boot("addi zero, zero, 5\nebreak\n");
        let retired = cpu.step();
        assert_eq!(read(&cpu, "zero"), 0);
        assert_eq!(retired[0].register_write, None);
    }

    #[test]
    fn loads_and_stores_with_sign_extension() {
        let cpu = run(".data\nv: .word 0x8081f0ff\n.text\n\
             la t0, v\n\
             lb a0, 0(t0)\n\
             lbu a1, 0(t0)\n\
             lh a2, 2(t0)\n\
             lhu a3, 2(t0)\n\
             lw a4, 0(t0)\n\
             li t1, 0x11\n\
             sb t1, 1(t0)\n\
             sh t1, -4(sp)\n\
             lw a5, 0(t0)\n\
             lw a6, -4(sp)\n\
             ebreak\n");
        assert_eq!(read(&cpu, "a0"), 0xffff_ffff);
        assert_eq!(read(&cpu, "a1"), 0xff);
        assert_eq!(read(&cpu, "a2"), 0xffff_8081);
        assert_eq!(read(&cpu, "a3"), 0x8081);
        assert_eq!(read(&cpu, "a4"), 0x8081_f0ff);
        assert_eq!(read(&cpu, "a5"), 0x8081_11ff);
        assert_eq!(read(&cpu, "a6"), 0x11);
    }

    #[test]
    fn retirement_records_what_changed() {
        let mut cpu = boot("li t0, 7\nsw t0, -8(sp)\nlw t1, -8(sp)\nebreak\n");
        let li = cpu.step()[0];
        assert_eq!(
            li.register_write,
            Some(RegisterWrite {
                reg: reg("t0"),
                value: 7
            })
        );
        assert_eq!((li.pc, li.next_pc), (0, 4));
        let sw = cpu.step()[0];
        assert_eq!(sw.register_write, None);
        assert_eq!(
            sw.data_access,
            Some(DataAccess {
                kind: AccessKind::Store,
                address: 0x7fff_ffe8,
                width: Width::Word,
                value: 7
            })
        );
        let lw = cpu.step()[0];
        assert_eq!(
            lw.data_access.map(|access| access.kind),
            Some(AccessKind::Load)
        );
        assert_eq!(
            cpu.stats(),
            Stats {
                cycles: 3,
                retired: 3,
                ..Stats::default()
            }
        );
    }

    #[test]
    fn branches_and_loops() {
        // Suma de 1 a 10 con un bucle y saltos con y sin signo.
        let cpu = run("li t0, 10\nli a0, 0\n\
             loop: add a0, a0, t0\n\
             addi t0, t0, -1\n\
             bnez t0, loop\n\
             li t1, -1\n\
             li a1, 0\n\
             bltu t1, zero, skip\n\
             li a1, 1\n\
             skip: blt t1, zero, end\n\
             li a1, 99\n\
             end: ebreak\n");
        assert_eq!(read(&cpu, "a0"), 55);
        assert_eq!(read(&cpu, "a1"), 1);
    }

    #[test]
    fn calls_link_and_return() {
        let cpu = run("main: call f\n\
             auipc a1, 0\n\
             ebreak\n\
             f: li a0, 42\n\
             ret\n");
        assert_eq!(read(&cpu, "a0"), 42);
        // call = auipc + jalr (8 bytes): ra apunta a la instrucción siguiente.
        assert_eq!(read(&cpu, "ra"), 8);
        assert_eq!(read(&cpu, "a1"), 8);
    }

    #[test]
    fn recursive_factorial_example() {
        let source = include_str!("../../../examples/factorial.s");
        let mut cpu = boot(source);
        let outcome = cpu.run(10_000);
        assert!(matches!(
            outcome,
            RunOutcome::Halted(Halt {
                trap: Trap::Exited { code: 0 },
                ..
            })
        ));
        assert_eq!(read(&cpu, "a0"), 720);
        assert_eq!(read(&cpu, "sp"), crate::map::INITIAL_SP);
    }

    #[test]
    fn faulting_instruction_leaves_state_untouched() {
        let mut cpu = boot("li a0, 1\nlw a0, 2(sp)\nnop\n");
        cpu.step();
        let before = cpu.state().clone();
        assert!(cpu.step().is_empty());
        assert_eq!(cpu.state(), &before);
        assert_eq!(
            cpu.halt(),
            Some(Halt {
                pc: 4,
                trap: Trap::Memory(MemoryError::Misaligned {
                    address: 0x7fff_fff2,
                    width: Width::Word,
                    access: AccessKind::Load
                })
            })
        );
        assert!(cpu.step().is_empty());
        assert_eq!(cpu.stats().retired, 1);
    }

    #[test]
    fn running_off_the_end_of_the_code_stops() {
        let mut cpu = boot("nop\n");
        let outcome = cpu.run(10);
        assert_eq!(
            outcome,
            RunOutcome::Halted(Halt {
                pc: 4,
                trap: Trap::Memory(MemoryError::Unmapped {
                    address: 4,
                    access: AccessKind::Fetch
                })
            })
        );
    }

    #[test]
    fn misaligned_jump_targets_trap_at_the_jump() {
        let mut cpu = boot("li t0, 6\njalr ra, 0(t0)\nnop\nnop\n");
        let outcome = cpu.run(10);
        assert_eq!(
            outcome,
            RunOutcome::Halted(Halt {
                pc: 4,
                trap: Trap::MisalignedTarget { target: 6 }
            })
        );
        assert_eq!(read(&cpu, "ra"), 0);
        // jalr limpia el bit 0: saltar a 9 es saltar a 8.
        let cpu = run("li t0, 9\njalr ra, 0(t0)\nebreak\n");
        assert_eq!(read(&cpu, "ra"), 8);
    }

    #[test]
    fn illegal_words_and_writes_to_code_stop_with_a_message() {
        let mut cpu = boot(".word 0xffffffff\n");
        cpu.run(1);
        assert_eq!(
            cpu.halt().map(|halt| halt.trap),
            Some(Trap::Decode(DecodeError::Illegal(0xffff_ffff)))
        );
        let mut cpu = boot("sw zero, 0(zero)\n");
        cpu.run(1);
        let message = cpu.halt().map(|halt| halt.to_string());
        assert_eq!(
            message.as_deref(),
            Some(
                "0x00000000: no se puede escribir en 0x00000000: el segmento de texto no lo permite"
            )
        );
    }

    #[test]
    fn infinite_loops_hit_the_cycle_limit() {
        let mut cpu = boot("loop: j loop\n");
        assert_eq!(cpu.run(1_000), RunOutcome::CycleLimit);
        assert_eq!(cpu.stats().cycles, 1_000);
    }

    #[test]
    fn exit_ecall_retires_and_stops() {
        let mut cpu = boot("li a0, 3\nli a7, 93\necall\nnop\n");
        let outcome = cpu.run(100);
        assert_eq!(
            outcome,
            RunOutcome::Halted(Halt {
                pc: 8,
                trap: Trap::Exited { code: 3 }
            })
        );
        assert_eq!(cpu.stats().retired, 3);
        assert_eq!(cpu.state().pc, 12);
        assert_eq!(
            cpu.halt().map(|halt| halt.to_string()).as_deref(),
            Some("0x00000008: el programa terminó con código 3")
        );
    }

    const ECHO: &str = ".data\nprompt: .string \"n? \"\n.text\n\
         la a0, prompt\nli a7, 4\necall\n\
         li a7, 5\necall\n\
         slli a0, a0, 1\nli a7, 1\necall\n\
         li a0, 10\nli a7, 11\necall\n\
         li a7, 10\necall\n";

    #[test]
    fn reading_waits_for_input_without_spending_cycles() {
        let mut cpu = boot(ECHO);
        assert_eq!(cpu.run(100), RunOutcome::AwaitingInput);
        assert!(cpu.awaiting_input());
        let waiting = cpu.clone();
        assert_eq!(cpu.run(100), RunOutcome::AwaitingInput);
        assert!(cpu.step().is_empty());
        assert_eq!(cpu.stats(), waiting.stats());
        assert_eq!(cpu.state(), waiting.state());

        cpu.console_mut().push_input("21\n");
        let read = cpu.step();
        assert!(!cpu.awaiting_input());
        assert_eq!(
            read[0].register_write,
            Some(RegisterWrite {
                reg: Reg::A0,
                value: 21
            })
        );
        assert!(matches!(cpu.run(100), RunOutcome::Halted(_)));
        assert_eq!(cpu.state().console.output_text(), "n? 42\n");

        // Con la entrada dada desde el principio, la traza es la misma.
        let mut eager = boot(ECHO);
        eager.console_mut().push_input("21\n");
        eager.run(100);
        assert_eq!(eager.state(), cpu.state());
        assert_eq!(eager.stats(), cpu.stats());
    }

    #[test]
    fn bad_ecalls_stop_at_the_ecall() {
        let mut cpu = boot("li a7, 7\necall\n");
        assert_eq!(
            cpu.run(10),
            RunOutcome::Halted(Halt {
                pc: 4,
                trap: Trap::UnknownSyscall { number: 7 }
            })
        );
        let mut cpu = boot("li a7, 5\necall\n");
        cpu.console_mut().close_input();
        cpu.run(10);
        assert_eq!(cpu.halt().map(|halt| halt.trap), Some(Trap::EndOfInput));
        assert_eq!(cpu.stats().retired, 1);
    }

    #[test]
    fn execution_is_deterministic() {
        let source = include_str!("../../../examples/factorial.s");
        let mut first = boot(source);
        let mut second = boot(source);
        let mut trace_first = Vec::new();
        let mut trace_second = Vec::new();
        for _ in 0..200 {
            trace_first.extend(first.step());
            trace_second.extend(second.step());
        }
        assert_eq!(trace_first, trace_second);
        assert_eq!(first, second);
    }
}
