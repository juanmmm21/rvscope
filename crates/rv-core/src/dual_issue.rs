//! Procesador superescalar en orden de doble emisión: dos vías de cinco etapas con forwarding
//! que emiten hasta dos instrucciones por ciclo cuando son independientes. La vía 0 lleva
//! siempre la más antigua. Las reglas de emparejamiento, búsqueda y paradas están en el
//! ADR 0003; lo que no cambia respecto al pipeline de cinco etapas, en el ADR 0002.

use rv_asm::Program;
use rv_isa::Reg;

use crate::exec::{Operation, execute};
use crate::map::MapError;
use crate::pipeline::{
    Bypass, Fetched, Latch, Uop, WriteBack, decode, fetch, memory_stage, write_back,
};
use crate::processor::{Halt, Processor, Retired, Stats, Trap};
use crate::state::ArchState;
use crate::syscall::Console;
use crate::view::{Forwarded, PipelineView, Slot, SlotState, Stage};

/// Lo que lleva cada registro de segmentación: un hueco por vía.
type Lanes<T> = [Latch<T>; 2];

const EMPTY: Lanes<Uop> = [Latch::Bubble, Latch::Bubble];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DualIssue {
    state: ArchState,
    stats: Stats,
    view: PipelineView,
    halt: Option<Halt>,
    awaiting_input: bool,
    /// `pc` de la siguiente búsqueda: el de detrás de lo último que hay en IF/ID.
    fetch_pc: u32,
    /// Ventana de búsqueda: instrucciones consecutivas, la más antigua en la vía 0.
    if_id: Lanes<Fetched>,
    id_ex: Lanes<Uop>,
    ex_mem: Lanes<Uop>,
    mem_wb: Lanes<Uop>,
}

/// Cierto si es un salto, `jal` o `jalr`: cierra el par en que se emite.
fn is_control(latch: &Latch<Uop>) -> bool {
    matches!(latch, Latch::Holds(uop) if matches!(
        uop.operation,
        Operation::Branch(_) | Operation::Jal | Operation::Jalr
    ))
}

fn accesses_memory(latch: &Latch<Uop>) -> bool {
    matches!(latch, Latch::Holds(uop) if matches!(
        uop.operation,
        Operation::Load(_) | Operation::Store(_)
    ))
}

/// Cierto si la instrucción lee `written` como operando. `written` nunca es `x0`.
fn reads(latch: &Latch<Uop>, written: Option<Reg>) -> bool {
    match (latch, written) {
        (Latch::Holds(uop), Some(reg)) => uop.instruction.rs1 == reg || uop.instruction.rs2 == reg,
        _ => false,
    }
}

impl DualIssue {
    pub fn new(program: &Program) -> Result<DualIssue, MapError> {
        Ok(DualIssue::from_state(ArchState::for_program(program)?))
    }

    /// Arranca con el pipeline vacío desde un estado ya preparado.
    pub fn from_state(state: ArchState) -> DualIssue {
        DualIssue {
            fetch_pc: state.pc,
            state,
            stats: Stats::default(),
            view: PipelineView::empty(&Stage::PIPELINE, 2),
            halt: None,
            awaiting_input: false,
            if_id: [Latch::Bubble, Latch::Bubble],
            id_ex: EMPTY,
            ex_mem: EMPTY,
            mem_wb: EMPTY,
        }
    }

    /// Valor de `reg` para EX: el de la instrucción más reciente de las que van delante y lo
    /// escriben (de EX/MEM a MEM/WB y, dentro de cada uno, la vía 1 antes que la 0), o `read`.
    fn forward(&self, reg: Reg, read: u32) -> (u32, Option<Forwarded>) {
        let sources = [
            (&self.ex_mem[1], (Bypass::ExMem, 1)),
            (&self.ex_mem[0], (Bypass::ExMem, 0)),
            (&self.mem_wb[1], (Bypass::MemWb, 1)),
            (&self.mem_wb[0], (Bypass::MemWb, 0)),
        ];
        for (latch, source) in sources {
            if let Some(produced) = latch.produces(reg) {
                return match produced {
                    Some(value) => (value, Some(source)),
                    None => (read, None),
                };
            }
        }
        (read, None)
    }

    /// EX de una vía. Devuelve también de dónde vino cada operando y el destino si salta.
    fn execute_lane(&self, latch: Latch<Uop>) -> (Latch<Uop>, [Option<Forwarded>; 2], Option<u32>) {
        let Latch::Holds(mut uop) = latch else {
            return (latch, [None; 2], None);
        };
        let (rs1, rs1_bypass) = self.forward(uop.instruction.rs1, uop.rs1);
        let (rs2, rs2_bypass) = self.forward(uop.instruction.rs2, uop.rs2);
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

    /// Cierto si `decoded` lee lo que carga una instrucción que está en EX en este ciclo.
    fn load_use(&self, decoded: &Latch<Uop>) -> bool {
        self.id_ex
            .iter()
            .any(|producer| producer.holds_load() && reads(decoded, producer.destination()))
    }

    /// Cuántas instrucciones de la ventana se emiten (0, 1 o 2), según el ADR 0003.
    fn issue_count(&self, decoded: &Lanes<Uop>) -> usize {
        let [older, younger] = decoded;
        if matches!(older, Latch::Bubble) || self.load_use(older) {
            return 0;
        }
        let alone = |latch: &Latch<Uop>| latch.blocks_fetch();
        let pairs = matches!(younger, Latch::Holds(_))
            && !alone(older)
            && !alone(younger)
            && !is_control(older)
            && !(accesses_memory(older) && accesses_memory(younger))
            && !reads(younger, older.destination())
            && !self.load_use(younger);
        if pairs { 2 } else { 1 }
    }

    /// Vista del ciclo que acaba de terminar: por etapa de IF a WB, un hueco por vía.
    fn record(&mut self, slots: [[Option<Slot>; 2]; 5]) {
        self.view = PipelineView::empty(&Stage::PIPELINE, 2);
        self.view.cycle = self.stats.cycles;
        for (stage, lanes) in self.view.stages.iter_mut().zip(slots) {
            stage.lanes = lanes.to_vec();
        }
    }

    /// Para el procesador. En la vista queda solo WB: lo que se ha retirado, la instrucción
    /// que para y, si la que para es la de la vía 0, la de la vía 1 descartada.
    fn stop(&mut self, halt: Halt, retiring: [Option<Slot>; 2]) {
        let mut reached = true;
        let retiring = retiring.map(|slot| {
            slot.map(|mut slot| {
                if !reached {
                    slot.state = SlotState::Flushed;
                } else if slot.pc == halt.pc {
                    slot.trap = Some(halt.trap);
                    reached = false;
                }
                slot
            })
        });
        self.record([
            [None, None],
            [None, None],
            [None, None],
            [None, None],
            retiring,
        ]);
        self.halt = Some(halt);
        self.if_id = [Latch::Bubble, Latch::Bubble];
        self.id_ex = EMPTY;
        self.ex_mem = EMPTY;
        self.mem_wb = EMPTY;
    }

    /// WB de las dos vías, la 0 primero. Devuelve lo retirado y, si hay que parar, por qué;
    /// `None` si una `ecall` espera entrada (no ha cambiado nada: va siempre sola en la vía 0).
    fn write_back_stage(&mut self) -> Option<(Vec<Retired>, Option<Halt>)> {
        let mut retired = Vec::new();
        for latch in self.mem_wb {
            match latch {
                Latch::Bubble => {}
                Latch::Fault(halt) => return Some((retired, Some(halt))),
                Latch::Holds(uop) => match write_back(&mut self.state, &uop) {
                    Ok(WriteBack::Retired {
                        retired: entry,
                        exit,
                    }) => {
                        retired.push(entry);
                        if let Some(code) = exit {
                            let halt = Halt {
                                pc: uop.pc,
                                trap: Trap::Exited { code },
                            };
                            return Some((retired, Some(halt)));
                        }
                    }
                    Ok(WriteBack::AwaitingInput) => return None,
                    Err(trap) => return Some((retired, Some(Halt { pc: uop.pc, trap }))),
                },
            }
        }
        Some((retired, None))
    }

    /// IF: rellena los huecos libres de la ventana detrás de lo que queda. Tras un fallo de
    /// búsqueda no se busca más: esa instrucción para el procesador si llega a WB.
    fn fill_window(&mut self, remaining: &[Latch<Fetched>]) -> (Lanes<Fetched>, [bool; 2]) {
        let mut window = [Latch::Bubble, Latch::Bubble];
        let mut fetched = [false; 2];
        window[..remaining.len()].copy_from_slice(remaining);
        for slot in remaining.len()..2 {
            let pc = self.fetch_pc;
            let latch = fetch(&self.state.memory, pc);
            window[slot] = latch;
            fetched[slot] = true;
            match latch {
                Latch::Holds(_) => self.fetch_pc = pc.wrapping_add(4),
                Latch::Bubble | Latch::Fault(_) => break,
            }
        }
        (window, fetched)
    }
}

impl Processor for DualIssue {
    fn name(&self) -> &'static str {
        "doble emisión en orden (2 vías de 5 etapas con forwarding)"
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

    /// Las etapas se evalúan de WB hacia IF, como en el pipeline de cinco etapas, y dentro de
    /// cada etapa la vía 0 antes que la 1.
    fn step(&mut self) -> Vec<Retired> {
        if self.halt.is_some() {
            return Vec::new();
        }
        let retiring = self
            .mem_wb
            .map(|latch| latch.slot(Stage::Wb, SlotState::Active, [None; 2]));

        let Some((retired, halt)) = self.write_back_stage() else {
            self.awaiting_input = true;
            return Vec::new();
        };
        self.awaiting_input = false;
        // El ciclo cuenta si algo se retira, aunque luego se pare.
        if halt.is_none() || !retired.is_empty() {
            self.stats.cycles += 1;
            self.stats.retired += retired.len() as u64;
        }
        let retiring = retiring.map(|slot| {
            slot.map(|mut slot| {
                if let Some(entry) = retired.iter().find(|entry| entry.pc == slot.pc) {
                    slot.retire(entry);
                }
                slot
            })
        });
        if let Some(halt) = halt {
            self.stop(halt, retiring);
            return retired;
        }

        // MEM: si la vía 0 falla, la 1 (más joven) no llega a tocar la memoria.
        let first = memory_stage(&mut self.state.memory, self.ex_mem[0]);
        let second = match first {
            Latch::Fault(_) => Latch::Bubble,
            _ => memory_stage(&mut self.state.memory, self.ex_mem[1]),
        };
        let mem_wb = [first, second];

        // EX: un salto cierra su par, así que como mucho una vía pide redirigir.
        let (first, first_bypasses, first_redirect) = self.execute_lane(self.id_ex[0]);
        let (second, second_bypasses, second_redirect) = self.execute_lane(self.id_ex[1]);
        let ex_mem = [first, second];
        let executing = [
            first.slot(Stage::Ex, SlotState::Active, first_bypasses),
            second.slot(Stage::Ex, SlotState::Active, second_bypasses),
        ];
        let redirect = first_redirect.or(second_redirect);

        let in_flight = ex_mem.iter().chain(&mem_wb).any(Latch::blocks_fetch);
        let decoded = self.if_id.map(|latch| decode(latch, &self.state.registers));
        let (id_ex, if_id, decoding, fetching) = match redirect {
            Some(target) => {
                self.fetch_pc = target;
                let decoding =
                    decoded.map(|latch| latch.slot(Stage::Id, SlotState::Flushed, [None; 2]));
                self.stats.flushed += decoding.iter().flatten().count() as u64;
                (
                    EMPTY,
                    [Latch::Bubble, Latch::Bubble],
                    decoding,
                    [None, None],
                )
            }
            None => {
                // Detrás de una `ecall` o de un fallo en vuelo no se emite nada: lo de detrás
                // leería registros que la `ecall` aún puede cambiar.
                let issued = if in_flight {
                    0
                } else {
                    self.issue_count(&decoded)
                };
                if issued == 0 && !in_flight && matches!(decoded[0], Latch::Holds(_)) {
                    self.stats.stalls += 1;
                }
                if issued == 2 {
                    self.stats.dual_issues += 1;
                }
                let mut decoding = [None, None];
                for (lane, latch) in decoded.iter().enumerate() {
                    let state = if lane < issued {
                        SlotState::Active
                    } else {
                        SlotState::Stalled
                    };
                    decoding[lane] = latch.slot(Stage::Id, state, [None; 2]);
                }
                let mut id_ex = EMPTY;
                id_ex[..issued].copy_from_slice(&decoded[..issued]);
                let remaining: Vec<Latch<Fetched>> = self.if_id[issued..]
                    .iter()
                    .copied()
                    .filter(|latch| !matches!(latch, Latch::Bubble))
                    .collect();
                let blocked = in_flight || decoded.iter().any(Latch::blocks_fetch);
                if blocked {
                    let mut window = [Latch::Bubble, Latch::Bubble];
                    window[..remaining.len()].copy_from_slice(&remaining);
                    (id_ex, window, decoding, [None, None])
                } else {
                    let (window, fetched) = self.fill_window(&remaining);
                    let fetching = [0, 1].map(|lane| {
                        if fetched[lane] {
                            window[lane].slot(SlotState::Active)
                        } else {
                            None
                        }
                    });
                    (id_ex, window, decoding, fetching)
                }
            }
        };
        self.record([
            fetching,
            decoding,
            executing,
            mem_wb.map(|latch| latch.slot(Stage::Mem, SlotState::Active, [None; 2])),
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
    use crate::five_stage::{FiveStage, Hazards};
    use crate::memory::{AccessKind, MemoryError, Width};
    use crate::processor::RunOutcome;
    use crate::single_cycle::SingleCycle;
    use crate::view::Operand;

    const EXIT: &str = "li a7, 10\necall\n";

    fn program(source: &str) -> Program {
        rv_asm::assemble(source).unwrap()
    }

    /// `pc` de lo que hay en cada etapa (IF, ID, EX, MEM, WB) y vía según la vista.
    fn pcs(cpu: &DualIssue) -> [[Option<u32>; 2]; 5] {
        let stages = &cpu.view().stages;
        std::array::from_fn(|stage| {
            std::array::from_fn(|lane| stages[stage].lanes[lane].map(|slot| slot.pc))
        })
    }

    /// De dónde (registro de segmentación y vía) tomó EX `rs1` y `rs2` en cada vía en el
    /// último ciclo.
    fn bypasses(cpu: &DualIssue) -> [[Option<Forwarded>; 2]; 2] {
        let ex = &cpu.view().stages[2];
        let bypass = |operand: Option<Operand>| {
            operand.and_then(|operand| operand.bypass.zip(operand.bypass_lane))
        };
        std::array::from_fn(|lane| {
            ex.lanes[lane].map_or([None; 2], |slot| [bypass(slot.rs1), bypass(slot.rs2)])
        })
    }

    fn boot(source: &str) -> DualIssue {
        DualIssue::new(&program(source)).unwrap()
    }

    fn read(cpu: &impl Processor, name: &str) -> u32 {
        cpu.state().registers.read(Reg::parse(name).unwrap())
    }

    /// Ejecuta hasta pararse y devuelve lo retirado en cada ciclo.
    fn cycles(cpu: &mut impl Processor) -> Vec<Vec<Retired>> {
        let mut cycles = Vec::new();
        for _ in 0..10_000 {
            if cpu.halt().is_some() {
                break;
            }
            cycles.push(cpu.step());
        }
        assert!(cpu.halt().is_some(), "no se ha parado");
        cycles
    }

    /// Retira lo mismo, en el mismo orden, y acaba igual que el de un ciclo.
    fn assert_matches_single_cycle(source: &str) -> DualIssue {
        let program = program(source);
        let mut reference = SingleCycle::new(&program).unwrap();
        let mut cpu = DualIssue::new(&program).unwrap();
        let retired: Vec<Retired> = cycles(&mut cpu).into_iter().flatten().collect();
        let expected: Vec<Retired> = cycles(&mut reference).into_iter().flatten().collect();
        assert_eq!(retired, expected);
        assert_eq!(cpu.state(), reference.state());
        assert_eq!(cpu.halt(), reference.halt());
        assert_eq!(cpu.stats().retired, reference.stats().retired);
        cpu
    }

    /// Lo que entró en EX cada ciclo (vía 0, vía 1), sin los ciclos en que no entró nada.
    fn executed(source: &str) -> Vec<[Option<u32>; 2]> {
        let mut cpu = boot(source);
        // Sin entrada, una `ecall` de lectura para en vez de esperar.
        cpu.console_mut().close_input();
        let mut groups = Vec::new();
        for _ in 0..10_000 {
            if cpu.halt().is_some() {
                break;
            }
            cpu.step();
            let ex = pcs(&cpu)[2];
            if ex != [None, None] {
                groups.push(ex);
            }
        }
        groups
    }

    #[test]
    fn independent_instructions_issue_and_retire_in_pairs() {
        let source = format!("li a0, 1\nli a1, 2\nli a2, 3\nli a3, 4\nli a4, 5\nli a5, 6\n{EXIT}");
        let mut cpu = assert_matches_single_cycle(&source);
        assert_eq!(cpu.stats().dual_issues, 3);
        assert_eq!(
            executed(&source),
            [
                [Some(0), Some(4)],
                [Some(8), Some(12)],
                [Some(16), Some(20)],
                [Some(24), None],
                [Some(28), None],
            ]
        );
        let mut cpu2 = boot(&source);
        let per_cycle = cycles(&mut cpu2);
        let pcs: Vec<u32> = per_cycle[4].iter().map(|entry| entry.pc).collect();
        assert_eq!(pcs, [0, 4], "el quinto ciclo retira el primer par");
        assert!(per_cycle.iter().all(|retired| retired.len() <= 2));
        cpu.run(10);
        assert_eq!(
            cpu.name(),
            "doble emisión en orden (2 vías de 5 etapas con forwarding)"
        );
    }

    #[test]
    fn a_dependent_younger_waits_for_the_next_cycle() {
        let source = format!("li a0, 1\naddi a1, a0, 1\n{EXIT}");
        let cpu = assert_matches_single_cycle(&source);
        assert_eq!(read(&cpu, "a1"), 2);
        assert_eq!(cpu.stats().stalls, 0);
        assert_eq!(
            executed(&source),
            [[Some(0), None], [Some(4), Some(8)], [Some(12), None]]
        );

        let mut cpu = boot(&source);
        for _ in 0..4 {
            cpu.step();
        }
        assert_eq!(pcs(&cpu)[2], [Some(4), Some(8)]);
        assert_eq!(
            bypasses(&cpu),
            [[Some((Bypass::ExMem, 0)), None], [None, None]]
        );
    }

    #[test]
    fn the_view_says_which_lane_a_bypass_comes_from() {
        let source = format!("li a0, 1\nli a1, 2\nadd a2, a1, a0\n{EXIT}");
        let cpu = assert_matches_single_cycle(&source);
        assert_eq!(read(&cpu, "a2"), 3);

        let mut cpu = boot(&source);
        for _ in 0..4 {
            cpu.step();
        }
        assert_eq!(pcs(&cpu)[2], [Some(8), Some(12)]);
        // `a1` lo escribe la joven del par de delante y `a0` la antigua.
        assert_eq!(
            bypasses(&cpu),
            [
                [Some((Bypass::ExMem, 1)), Some((Bypass::ExMem, 0))],
                [None, None]
            ]
        );
    }

    #[test]
    fn one_memory_port_and_branches_close_the_pair() {
        let stores = format!("sw zero, 0(sp)\nsw zero, 4(sp)\nlw a0, 8(sp)\n{EXIT}");
        assert_matches_single_cycle(&stores);
        assert_eq!(
            executed(&stores),
            [
                [Some(0), None],
                [Some(4), None],
                [Some(8), Some(12)],
                [Some(16), None],
            ]
        );

        // El salto va solo si es el más antiguo; lo de detrás se vacía en ID e IF.
        let older = format!("beq zero, zero, skip\nli a0, 1\nskip: li a1, 2\n{EXIT}");
        let cpu = assert_matches_single_cycle(&older);
        assert_eq!(read(&cpu, "a0"), 0);
        assert_eq!(executed(&older)[0], [Some(0), None]);
        assert_eq!(executed(&older)[1], [Some(8), Some(12)]);

        // Como más joven sí acompaña a la de delante.
        let younger = format!("li a2, 3\nbeq zero, zero, skip\nli a0, 1\nskip: li a1, 2\n{EXIT}");
        assert_matches_single_cycle(&younger);
        assert_eq!(executed(&younger)[0], [Some(0), Some(4)]);
        assert_eq!(executed(&younger)[1], [Some(12), Some(16)]);
    }

    #[test]
    fn load_use_costs_one_bubble_in_either_lane() {
        let source = format!("lw t0, 0(sp)\naddi t1, t0, 1\n{EXIT}");
        let cpu = assert_matches_single_cycle(&source);
        assert_eq!(cpu.stats().stalls, 1);

        // La carga va en un par y quien la usa llega justo detrás: también espera.
        let paired = format!("li t2, 5\nlw t0, 0(sp)\naddi t1, t0, 1\nli a1, 2\n{EXIT}");
        let cpu = assert_matches_single_cycle(&paired);
        assert_eq!(cpu.stats().stalls, 1);
        assert_eq!(
            executed(&paired)[..2],
            [[Some(0), Some(4)], [Some(8), Some(12)]]
        );
    }

    #[test]
    fn same_destination_pairs_keep_the_younger_value() {
        let source = format!("li a0, 1\nli a0, 2\naddi a1, a0, 0\nadd a2, a0, a1\n{EXIT}");
        let cpu = assert_matches_single_cycle(&source);
        assert_eq!(executed(&source)[0], [Some(0), Some(4)]);
        assert_eq!(
            (read(&cpu, "a0"), read(&cpu, "a1"), read(&cpu, "a2")),
            (2, 2, 4)
        );
    }

    #[test]
    fn traps_are_precise_across_lanes() {
        let misaligned = Trap::Memory(MemoryError::Misaligned {
            address: 0x7fff_fff2,
            width: Width::Word,
            access: AccessKind::Load,
        });
        // Falla la más joven: la de la vía 0 se retira en ese mismo ciclo.
        let source = "li a0, 7\nlw a1, 2(sp)\nli a2, 9\n";
        let mut cpu = assert_matches_single_cycle(source);
        assert_eq!(
            cpu.halt(),
            Some(Halt {
                pc: 4,
                trap: misaligned
            })
        );
        assert_eq!((read(&cpu, "a0"), read(&cpu, "a2")), (7, 0));
        let last = cycles(&mut boot(source)).pop().unwrap();
        assert_eq!(last.len(), 1);
        assert_eq!(cpu.stats().cycles, 5);
        assert!(cpu.step().is_empty());

        // Falla la más antigua: la de la vía 1 no escribe nada.
        let cpu = assert_matches_single_cycle("lw a1, 2(sp)\nli a0, 7\n");
        assert_eq!(read(&cpu, "a0"), 0);
        assert_eq!(cpu.stats().retired, 0);

        assert_matches_single_cycle(".word 0xffffffff\nli a0, 1\n");
        assert_matches_single_cycle("li a0, 1\n.word 0xffffffff\n");
        assert_matches_single_cycle("li a0, 1\n");
        assert_matches_single_cycle("li t0, 6\njalr ra, 0(t0)\nnop\n");
        assert_matches_single_cycle("li a0, 1\nebreak\nli a1, 2\n");
    }

    #[test]
    fn ecalls_go_alone_and_wait_for_input() {
        let source = ".data\nprompt: .string \"n? \"\n.text\n\
             la a0, prompt\nli a7, 4\necall\nli a7, 5\necall\nslli a0, a0, 1\nli a7, 1\n\
             ecall\nli a7, 10\necall\n";
        let mut cpu = boot(source);
        assert_eq!(cpu.run(1_000), RunOutcome::AwaitingInput);
        let waiting = cpu.stats();
        assert_eq!(cpu.run(1_000), RunOutcome::AwaitingInput);
        assert_eq!(cpu.stats(), waiting);
        cpu.console_mut().push_input("21\n");
        cpu.run(1_000);
        let mut reference = SingleCycle::new(&program(source)).unwrap();
        reference.console_mut().push_input("21\n");
        reference.run(1_000);
        assert_eq!(cpu.state(), reference.state());
        assert_eq!(cpu.state().console.output_text(), "n? 42");
        assert_eq!(cpu.stats().retired, reference.stats().retired);
        assert!(
            executed(source)
                .iter()
                .all(|[first, second]| second.is_none() || first.is_some())
        );
    }

    #[test]
    fn the_factorial_takes_fewer_cycles_than_the_scalar_pipeline() {
        let factorial = include_str!("../../../examples/factorial.s");
        let cpu = assert_matches_single_cycle(factorial);
        assert_eq!(read(&cpu, "a0"), 720);
        let mut scalar = FiveStage::new(&program(factorial), Hazards::Forward).unwrap();
        scalar.run(10_000);
        assert!(cpu.stats().cycles < scalar.stats().cycles);
        assert!(cpu.stats().dual_issues > 0);
    }

    #[test]
    fn execution_is_deterministic() {
        let factorial = include_str!("../../../examples/factorial.s");
        let mut first = boot(factorial);
        let mut second = boot(factorial);
        assert_eq!(cycles(&mut first), cycles(&mut second));
        assert_eq!(first, second);
    }
}
