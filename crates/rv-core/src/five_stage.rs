//! Procesador segmentado de cinco etapas (IF, ID, EX, MEM, WB). Según su unidad de riesgos, una
//! instrucción que lee un registro que alguna de las dos anteriores aún no ha escrito recibe el
//! valor viejo (`Hazards::Ignore`), espera en ID con burbujas hasta que se escribe
//! (`Hazards::Stall`) o lo recibe adelantado en EX y solo espera tras una carga
//! (`Hazards::Forward`). Los saltos se resuelven en EX y vacían lo que va detrás. Las
//! reglas de temporización y de paradas están en el ADR 0002.

use rv_asm::Program;
use rv_isa::Reg;

use crate::exec::execute;
use crate::map::MapError;
use crate::pipeline::{
    Bypass, Fetched, Latch, Uop, WriteBack, decode, fetch, memory_stage, write_back,
};
use crate::processor::{Halt, Processor, Retired, Stats, Trap};
use crate::state::ArchState;
use crate::syscall::Console;
use crate::view::{PipelineView, Slot, SlotState, Stage};

/// Unidad de riesgos de datos: lo único que cambia entre los modelos de cinco etapas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hazards {
    /// Sin detección: se lee lo que haya en el banco de registros, aunque sea viejo.
    Ignore,
    /// Detección en ID: si un operando lo escribe una instrucción que está en EX o en MEM, se
    /// congelan el `pc` e IF/ID y entra una burbuja en EX.
    Stall,
    /// Forwarding hacia EX desde EX/MEM y MEM/WB más detección de carga-uso: solo una carga en
    /// EX seguida de quien lee su destino en ID mete una burbuja.
    Forward,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FiveStage {
    hazards: Hazards,
    state: ArchState,
    stats: Stats,
    view: PipelineView,
    halt: Option<Halt>,
    awaiting_input: bool,
    /// `pc` de búsqueda; el arquitectónico es `state.pc`, que avanza al retirar.
    fetch_pc: u32,
    if_id: Latch<Fetched>,
    id_ex: Latch<Uop>,
    ex_mem: Latch<Uop>,
    mem_wb: Latch<Uop>,
}

impl FiveStage {
    pub fn new(program: &Program, hazards: Hazards) -> Result<FiveStage, MapError> {
        Ok(FiveStage::from_state(
            ArchState::for_program(program)?,
            hazards,
        ))
    }

    /// Arranca con el pipeline vacío desde un estado ya preparado.
    pub fn from_state(state: ArchState, hazards: Hazards) -> FiveStage {
        FiveStage {
            hazards,
            fetch_pc: state.pc,
            state,
            stats: Stats::default(),
            view: PipelineView::empty(&Stage::PIPELINE, 1),
            halt: None,
            awaiting_input: false,
            if_id: Latch::Bubble,
            id_ex: Latch::Bubble,
            ex_mem: Latch::Bubble,
            mem_wb: Latch::Bubble,
        }
    }

    pub fn hazards(&self) -> Hazards {
        self.hazards
    }

    /// Cierto si la instrucción de ID tiene que esperar. Sin forwarding, si lee un registro que
    /// escribe la que está en EX o la que está en MEM (la de WB ya lo ha escrito en la primera
    /// mitad del ciclo). Con forwarding, solo si lo escribe una carga que está en EX: su valor
    /// sale de MEM al final del ciclo siguiente, demasiado tarde para adelantarlo a EX.
    fn must_stall(&self, decoded: &Latch<Uop>) -> bool {
        let Latch::Holds(uop) = decoded else {
            return false;
        };
        let producers = match self.hazards {
            Hazards::Ignore => return false,
            Hazards::Stall => [self.id_ex.destination(), self.ex_mem.destination()],
            Hazards::Forward if self.id_ex.holds_load() => [self.id_ex.destination(), None],
            Hazards::Forward => return false,
        };
        let sources = [uop.instruction.rs1, uop.instruction.rs2];
        producers
            .into_iter()
            .flatten()
            .any(|written| sources.contains(&written))
    }

    /// Valor de `reg` para EX: el de la instrucción más reciente de las dos que van delante y
    /// lo escriben, o `read` (lo leído en ID) si ninguna lo hace. La de EX/MEM nunca es una
    /// carga sin valor: la burbuja de carga-uso lo impide.
    fn forward(&self, reg: Reg, read: u32) -> (u32, Option<Bypass>) {
        if self.hazards != Hazards::Forward {
            return (read, None);
        }
        for (latch, bypass) in [(&self.ex_mem, Bypass::ExMem), (&self.mem_wb, Bypass::MemWb)] {
            if let Some(produced) = latch.produces(reg) {
                return match produced {
                    Some(value) => (value, Some(bypass)),
                    None => (read, None),
                };
            }
        }
        (read, None)
    }

    /// Vista del ciclo que acaba de terminar, con un hueco por etapa de IF a WB.
    fn record(&mut self, slots: [Option<Slot>; 5]) {
        self.view = PipelineView::empty(&Stage::PIPELINE, 1);
        self.view.cycle = self.stats.cycles;
        for (stage, slot) in self.view.stages.iter_mut().zip(slots) {
            stage.lanes[0] = slot;
        }
    }

    /// Para el procesador. En la vista queda solo WB con la instrucción que termina o falla.
    fn stop(&mut self, halt: Halt, retiring: Option<Slot>) {
        let retiring = retiring.map(|mut slot| {
            if slot.pc == halt.pc {
                slot.trap = Some(halt.trap);
            }
            slot
        });
        self.record([None, None, None, None, retiring]);
        self.halt = Some(halt);
        self.if_id = Latch::Bubble;
        self.id_ex = Latch::Bubble;
        self.ex_mem = Latch::Bubble;
        self.mem_wb = Latch::Bubble;
    }

    /// EX: adelanta los operandos si hace falta, la ALU y la decisión de salto. Devuelve
    /// también de dónde vino cada operando y el destino si hay que vaciar. Los registros de
    /// segmentación de delante son los del principio del ciclo: aún no se han desplazado.
    fn execute_stage(&self) -> (Latch<Uop>, [Option<Bypass>; 2], Option<u32>) {
        let Latch::Holds(mut uop) = self.id_ex else {
            return (self.id_ex, [None; 2], None);
        };
        let (rs1, rs1_bypass) = self.forward(uop.instruction.rs1, uop.rs1);
        let (rs2, rs2_bypass) = self.forward(uop.instruction.rs2, uop.rs2);
        // El almacenamiento usa en MEM el `rs2` ya adelantado.
        uop.rs1 = rs1;
        uop.rs2 = rs2;
        let bypasses = [rs1_bypass, rs2_bypass];
        match execute(uop.operation, &uop.instruction, uop.pc, uop.rs1, uop.rs2) {
            Ok(execution) => {
                uop.next_pc = execution.next_pc;
                uop.result = execution.result;
                uop.address = execution.address;
                let redirect =
                    (execution.next_pc != uop.pc.wrapping_add(4)).then_some(execution.next_pc);
                (Latch::Holds(uop), bypasses, redirect)
            }
            Err(trap) => (Latch::Fault(Halt { pc: uop.pc, trap }), bypasses, None),
        }
    }

    /// IF: busca en `fetch_pc` y avanza si lo consigue.
    fn fetch_stage(&mut self) -> Latch<Fetched> {
        let fetched = fetch(&self.state.memory, self.fetch_pc);
        if let Latch::Holds(_) = fetched {
            self.fetch_pc = self.fetch_pc.wrapping_add(4);
        }
        fetched
    }
}

impl Processor for FiveStage {
    fn name(&self) -> &'static str {
        match self.hazards {
            Hazards::Ignore => "5 etapas sin forwarding ni detección de riesgos",
            Hazards::Stall => "5 etapas con detección de riesgos y burbujas",
            Hazards::Forward => "5 etapas con forwarding y detección de riesgos",
        }
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

    /// Las etapas se evalúan de WB hacia IF: así WB escribe antes de que ID lea, y una parada
    /// en WB impide que lo que va detrás toque la memoria.
    fn step(&mut self) -> Vec<Retired> {
        if self.halt.is_some() {
            return Vec::new();
        }
        // Se borra antes de nada: un ciclo que acaba en parada tampoco está esperando entrada.
        self.awaiting_input = false;
        let retiring = self.mem_wb.slot(Stage::Wb, SlotState::Active, [None; 2]);

        let mut retired = Vec::new();
        let mut exit = None;
        match self.mem_wb {
            Latch::Bubble => {}
            Latch::Fault(halt) => {
                self.stop(halt, retiring);
                return Vec::new();
            }
            Latch::Holds(uop) => match write_back(&mut self.state, &uop) {
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
                    self.stop(Halt { pc: uop.pc, trap }, retiring);
                    return Vec::new();
                }
            },
        }
        self.stats.cycles += 1;
        self.stats.retired += retired.len() as u64;
        let retiring = retiring.map(|mut slot| {
            if let Some(entry) = retired.first() {
                slot.retire(entry);
            }
            slot
        });
        if let Some((pc, code)) = exit {
            self.stop(
                Halt {
                    pc,
                    trap: Trap::Exited { code },
                },
                retiring,
            );
            return retired;
        }

        let mem_wb = memory_stage(&mut self.state.memory, self.ex_mem);
        let (ex_mem, bypasses, redirect) = self.execute_stage();
        let fetching_pc = self.fetch_pc;
        let (id_ex, if_id, decoding, fetching) = match redirect {
            Some(target) => {
                self.fetch_pc = target;
                // En un ciclo de vaciado IF e ID también trabajan, en el camino equivocado, y
                // la vista lo enseña aunque lo que hacen se descarte.
                let decoding = decode(self.if_id, &self.state.registers).slot(
                    Stage::Id,
                    SlotState::Flushed,
                    [None; 2],
                );
                let fetching = if mem_wb.blocks_fetch() {
                    None
                } else {
                    fetch(&self.state.memory, fetching_pc).slot(SlotState::Flushed)
                };
                self.stats.flushed += u64::from(decoding.is_some()) + u64::from(fetching.is_some());
                (Latch::Bubble, Latch::Bubble, decoding, fetching)
            }
            None => {
                let decoded = decode(self.if_id, &self.state.registers);
                if self.must_stall(&decoded) {
                    // La instrucción se queda en IF/ID y la de IF espera detrás; en EX entra
                    // una burbuja. `x0` nunca es destino, así que no frena a nadie.
                    self.stats.stalls += 1;
                    let decoding = decoded.slot(Stage::Id, SlotState::Stalled, [None; 2]);
                    let fetching = if ex_mem.blocks_fetch() || mem_wb.blocks_fetch() {
                        None
                    } else {
                        fetch(&self.state.memory, fetching_pc).slot(SlotState::Stalled)
                    };
                    (Latch::Bubble, self.if_id, decoding, fetching)
                } else {
                    let blocked =
                        decoded.blocks_fetch() || ex_mem.blocks_fetch() || mem_wb.blocks_fetch();
                    let if_id = if blocked {
                        Latch::Bubble
                    } else {
                        self.fetch_stage()
                    };
                    let decoding = decoded.slot(Stage::Id, SlotState::Active, [None; 2]);
                    (decoded, if_id, decoding, if_id.slot(SlotState::Active))
                }
            }
        };
        self.record([
            fetching,
            decoding,
            ex_mem.slot(
                Stage::Ex,
                SlotState::Active,
                bypasses.map(|bypass| bypass.map(|bypass| (bypass, 0))),
            ),
            mem_wb.slot(Stage::Mem, SlotState::Active, [None; 2]),
            retiring,
        ]);
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
    use crate::view::Operand;
    use rv_isa::DecodeError;

    fn program(source: &str) -> Program {
        rv_asm::assemble(source).unwrap()
    }

    /// `pc` de lo que hay en cada etapa (IF, ID, EX, MEM, WB) según la vista.
    fn pcs(cpu: &FiveStage) -> [Option<u32>; 5] {
        let stages = &cpu.view().stages;
        std::array::from_fn(|stage| stages[stage].lanes[0].map(|slot| slot.pc))
    }

    /// De dónde tomó EX `rs1` y `rs2` en el último ciclo.
    fn bypasses(cpu: &FiveStage) -> [Option<Bypass>; 2] {
        let ex = cpu.view().stages[2].lanes[0];
        let bypass = |operand: Option<Operand>| operand.and_then(|operand| operand.bypass);
        ex.map_or([None; 2], |slot| [bypass(slot.rs1), bypass(slot.rs2)])
    }

    fn boot(source: &str) -> FiveStage {
        FiveStage::new(&program(source), Hazards::Ignore).unwrap()
    }

    fn boot_stalling(source: &str) -> FiveStage {
        FiveStage::new(&program(source), Hazards::Stall).unwrap()
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
        assert_model_matches_single_cycle(source, Hazards::Ignore)
    }

    fn assert_model_matches_single_cycle(source: &str, hazards: Hazards) -> FiveStage {
        let program = program(source);
        let mut reference = SingleCycle::new(&program).unwrap();
        let mut pipelined = FiveStage::new(&program, hazards).unwrap();
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
        assert_eq!(pcs(&cpu), [Some(12), Some(8), Some(4), Some(0), None]);
        assert_eq!(cpu.step()[0].pc, 0);
        assert_eq!(pcs(&cpu), [Some(16), Some(12), Some(8), Some(4), Some(0)]);
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
                retired: 4,
                ..Stats::default()
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
        assert_eq!(pcs(&cpu), [Some(20), Some(16), Some(12), Some(8), Some(4)]);
        cpu.step();
        assert_eq!(pcs(&cpu), [Some(24), None, None, Some(12), Some(8)]);
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

    #[test]
    fn stalling_waits_for_the_producer_in_wb() {
        let source = "li a0, 5\naddi a1, a0, 1\nebreak\n";
        let mut cpu = boot_stalling(source);
        for _ in 0..2 {
            cpu.step();
        }
        // addi espera en ID mientras li está en EX y en MEM; IF la sigue reteniendo detrás.
        cpu.step();
        assert_eq!(pcs(&cpu), [Some(8), Some(4), Some(0), None, None]);
        cpu.step();
        assert_eq!(pcs(&cpu), [Some(8), Some(4), None, Some(0), None]);
        cpu.step();
        assert_eq!(pcs(&cpu), [Some(8), Some(4), None, None, Some(0)]);
        cpu.run(100);
        assert_eq!(read(&cpu, "a1"), 6);
        assert_eq!(cpu.stats().stalls, 2);
        // 2 retiradas + 4 de llenado + 2 burbujas; sin detección serían 6.
        assert_eq!(cpu.stats().cycles, 8);
        let mut unchecked = boot(source);
        unchecked.run(100);
        assert_eq!((unchecked.stats().cycles, unchecked.stats().stalls), (6, 0));
    }

    #[test]
    fn stalls_depend_on_the_distance_to_the_producer() {
        // Una instrucción en medio: solo falta una burbuja. Dos: ninguna.
        let mut cpu = boot_stalling("li a0, 5\nnop\naddi a1, a0, 1\nebreak\n");
        cpu.run(100);
        assert_eq!((read(&cpu, "a1"), cpu.stats().stalls), (6, 1));
        let mut cpu = boot_stalling("li a0, 5\nnop\nnop\naddi a1, a0, 1\nebreak\n");
        cpu.run(100);
        assert_eq!((read(&cpu, "a1"), cpu.stats().stalls), (6, 0));
    }

    #[test]
    fn only_registers_actually_read_and_written_stall() {
        // x0 no es destino, lui y jal no leen y un almacenamiento no escribe registro.
        let mut cpu = boot_stalling(
            "li zero, 5\nadd a0, zero, zero\nlui a1, 1\nlui a1, 2\n\
             sw a1, -4(sp)\nlw a2, -8(sp)\njal ra, end\nend: ebreak\n",
        );
        cpu.run(100);
        // Solo el sw espera al lui de delante (dos burbujas).
        assert_eq!(cpu.stats().stalls, 2);
        assert_eq!(read(&cpu, "a0"), 0);
    }

    #[test]
    fn stalling_fixes_every_data_hazard() {
        let cpu = assert_model_matches_single_cycle(
            "li a0, 5\naddi a1, a0, 1\naddi a2, a0, 1\naddi a3, a0, 1\nebreak\n",
            Hazards::Stall,
        );
        assert_eq!(
            (read(&cpu, "a1"), read(&cpu, "a2"), read(&cpu, "a3")),
            (6, 6, 6)
        );
        let cpu = assert_model_matches_single_cycle(
            "li t0, 7\nsw t0, -4(sp)\nlw a0, -4(sp)\nmv a1, a0\nbnez a1, end\nli a2, 1\n\
             end: ebreak\n",
            Hazards::Stall,
        );
        assert_eq!((read(&cpu, "a1"), read(&cpu, "a2")), (7, 0));
        let cpu = assert_model_matches_single_cycle(
            include_str!("../../../examples/factorial.s"),
            Hazards::Stall,
        );
        assert_eq!(read(&cpu, "a0"), 720);
    }

    #[test]
    fn stalling_keeps_traps_ecalls_and_hazard_free_timing() {
        let cpu = assert_model_matches_single_cycle(HAZARD_FREE, Hazards::Stall);
        let mut unchecked = boot(HAZARD_FREE);
        unchecked.run(10_000);
        assert_eq!((cpu.stats().stalls, cpu.stats()), (0, unchecked.stats()));

        assert_model_matches_single_cycle("li a0, 1\nlw a0, 2(sp)\nli a1, 7\n", Hazards::Stall);
        assert_model_matches_single_cycle("li t0, 6\njalr ra, 0(t0)\nnop\n", Hazards::Stall);

        let source = ".data\nprompt: .string \"n? \"\n.text\n\
             la a0, prompt\nli a7, 4\necall\nli a7, 5\necall\nslli a0, a0, 1\nli a7, 1\n\
             ecall\nli a7, 10\necall\n";
        let mut cpu = boot_stalling(source);
        assert_eq!(cpu.run(1_000), RunOutcome::AwaitingInput);
        cpu.console_mut().push_input("21\n");
        cpu.run(1_000);
        let mut reference = SingleCycle::new(&program(source)).unwrap();
        reference.console_mut().push_input("21\n");
        reference.run(1_000);
        assert_eq!(cpu.state(), reference.state());
        assert_eq!(cpu.state().console.output_text(), "n? 42");
        assert_eq!(cpu.name(), "5 etapas con detección de riesgos y burbujas");
    }

    fn boot_forwarding(source: &str) -> FiveStage {
        FiveStage::new(&program(source), Hazards::Forward).unwrap()
    }

    #[test]
    fn forwarding_feeds_alu_results_without_bubbles() {
        let source = "li a0, 5\naddi a1, a0, 1\naddi a2, a0, 2\nadd a3, a1, a2\nebreak\n";
        let mut cpu = boot_forwarding(source);
        for _ in 0..3 {
            cpu.step();
        }
        // li está en MEM y addi a1 en EX: a0 llega desde EX/MEM.
        assert_eq!(pcs(&cpu), [Some(8), Some(4), Some(0), None, None]);
        assert_eq!(bypasses(&cpu), [None, None]);
        cpu.step();
        assert_eq!(bypasses(&cpu), [Some(Bypass::ExMem), None]);
        cpu.step();
        // addi a2 toma a0 de li, que está en WB.
        assert_eq!(bypasses(&cpu), [Some(Bypass::MemWb), None]);
        cpu.step();
        // add a3 toma a1 de WB y a2 de MEM.
        assert_eq!(bypasses(&cpu), [Some(Bypass::MemWb), Some(Bypass::ExMem)]);
        cpu.run(100);
        assert_eq!(
            (read(&cpu, "a1"), read(&cpu, "a2"), read(&cpu, "a3")),
            (6, 7, 13)
        );
        assert_eq!(cpu.stats().stalls, 0);
        // 4 retiradas + 4 de llenado: como un programa sin dependencias.
        assert_eq!(cpu.stats().cycles, 8);
    }

    #[test]
    fn forwarding_prefers_the_most_recent_producer() {
        let cpu = assert_model_matches_single_cycle(
            "li a0, 1\nli a0, 2\naddi a1, a0, 0\nebreak\n",
            Hazards::Forward,
        );
        assert_eq!(read(&cpu, "a1"), 2);
        // x0 nunca se adelanta aunque alguien «lo escriba».
        let cpu = assert_model_matches_single_cycle(
            "addi zero, zero, 9\nadd a0, zero, zero\nebreak\n",
            Hazards::Forward,
        );
        assert_eq!(read(&cpu, "a0"), 0);
    }

    #[test]
    fn load_use_costs_exactly_one_bubble() {
        let source = "li t0, 7\nsw t0, -4(sp)\nlw a0, -4(sp)\naddi a1, a0, 1\nebreak\n";
        let mut cpu = boot_forwarding(source);
        for _ in 0..5 {
            cpu.step();
        }
        // lw en EX y addi esperando en ID.
        assert_eq!(pcs(&cpu), [Some(16), Some(12), Some(8), Some(4), Some(0)]);
        cpu.step();
        assert_eq!(pcs(&cpu), [Some(16), Some(12), None, Some(8), Some(4)]);
        cpu.step();
        // El valor cargado llega desde MEM/WB.
        assert_eq!(bypasses(&cpu), [Some(Bypass::MemWb), None]);
        cpu.run(100);
        assert_eq!(read(&cpu, "a1"), 8);
        assert_eq!(cpu.stats().stalls, 1);
        assert_eq!(cpu.stats().cycles, 4 + 4 + 1);

        // Con una instrucción en medio no hace falta esperar.
        let mut cpu =
            boot_forwarding("li t0, 7\nsw t0, -4(sp)\nlw a0, -4(sp)\nnop\nmv a1, a0\nebreak\n");
        cpu.run(100);
        assert_eq!((read(&cpu, "a1"), cpu.stats().stalls), (7, 0));
    }

    #[test]
    fn stores_and_branches_receive_forwarded_operands() {
        // El sw recibe t0 adelantado como dato y la dirección del addi de delante.
        let cpu = assert_model_matches_single_cycle(
            "li t0, 7\naddi t1, sp, -8\nsw t0, 0(t1)\nlw a0, -8(sp)\nbnez a0, end\n\
             li a1, 1\nend: li t2, 1\nbeq t2, zero, bad\nauipc t3, 0\njalr zero, 12(t3)\nbad: ebreak\nebreak\n",
            Hazards::Forward,
        );
        assert_eq!((read(&cpu, "a0"), read(&cpu, "a1")), (7, 0));
        // beq y jalr deciden con t2 y t3 adelantados: se para en el segundo ebreak.
        assert_eq!(cpu.halt().map(|halt| halt.pc), Some(read(&cpu, "t3") + 12));
        // Un enlace de jal también se adelanta.
        let cpu = assert_model_matches_single_cycle(
            "jal ra, next\nnext: addi a0, ra, 0\nebreak\n",
            Hazards::Forward,
        );
        assert_eq!(read(&cpu, "a0"), 4);
    }

    #[test]
    fn forwarding_matches_the_single_cycle_model_in_fewer_cycles() {
        let factorial = include_str!("../../../examples/factorial.s");
        let forwarding = assert_model_matches_single_cycle(factorial, Hazards::Forward);
        let stalling = assert_model_matches_single_cycle(factorial, Hazards::Stall);
        assert_eq!(read(&forwarding, "a0"), 720);
        assert!(forwarding.stats().cycles < stalling.stats().cycles);
        assert!(forwarding.stats().stalls < stalling.stats().stalls);

        let cpu = assert_model_matches_single_cycle(HAZARD_FREE, Hazards::Forward);
        let mut unchecked = boot(HAZARD_FREE);
        unchecked.run(10_000);
        assert_eq!(cpu.stats(), unchecked.stats());
        assert_model_matches_single_cycle("li a0, 1\nlw a0, 2(sp)\nli a1, 7\n", Hazards::Forward);
        assert_model_matches_single_cycle("li t0, 6\njalr ra, 0(t0)\nnop\n", Hazards::Forward);
        assert_eq!(cpu.name(), "5 etapas con forwarding y detección de riesgos");
    }

    #[test]
    fn forwarding_keeps_ecalls_in_step_with_the_single_cycle_model() {
        let source = ".data\nprompt: .string \"n? \"\n.text\n\
             la a0, prompt\nli a7, 4\necall\nli a7, 5\necall\nslli a0, a0, 1\nli a7, 1\n\
             ecall\nli a7, 10\necall\n";
        let mut cpu = boot_forwarding(source);
        assert_eq!(cpu.run(1_000), RunOutcome::AwaitingInput);
        cpu.console_mut().push_input("21\n");
        cpu.run(1_000);
        let mut reference = SingleCycle::new(&program(source)).unwrap();
        reference.console_mut().push_input("21\n");
        reference.run(1_000);
        assert_eq!(cpu.state(), reference.state());
        assert_eq!(cpu.state().console.output_text(), "n? 42");
    }

    #[test]
    fn other_models_never_forward() {
        for hazards in [Hazards::Ignore, Hazards::Stall] {
            let mut cpu =
                FiveStage::new(&program("li a0, 5\naddi a1, a0, 1\nebreak\n"), hazards).unwrap();
            for _ in 0..10 {
                cpu.step();
                assert_eq!(bypasses(&cpu), [None, None]);
            }
        }
    }
}
