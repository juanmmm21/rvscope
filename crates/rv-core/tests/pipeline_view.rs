//! La vista por etapa tiene que contar lo mismo que el procesador: lo que se retira aparece en
//! la última etapa con lo que escribe, cada instrucción pasa una vez por EX con los operandos
//! que usaría el de un ciclo (adelantados o no) y las estadísticas salen de lo que se ve.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::Path;

use rv_asm::{Layout, Program, assemble, assemble_with};
use rv_core::{
    AluFunction, DualIssue, FiveStage, Hazards, Operation, PcSource, PipelineTrace, PipelineView,
    Processor, SingleCycle, Slot, SlotState, Stage, Trap, Wires,
};

type Factory = fn(&Program) -> Box<dyn Processor>;

/// Modelos que acaban igual que el de un ciclo; el que ignora los riesgos se prueba aparte.
const PIPELINED: [(&str, Factory); 3] = [
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

const FACTORIAL: &str = include_str!("../../../examples/factorial.s");

/// Carga-uso, forwarding de las dos distancias, almacenamientos con el dato adelantado, saltos
/// hacia atrás y pares independientes para la doble emisión.
const MIXED: &str = ".data\nv: .word 4, 8, 15, 16, 23, 42\n.text\n\
     la t0, v\nli t1, 6\nli a0, 0\nli a1, 0\n\
     loop: lw t2, 0(t0)\n\
     add a0, a0, t2\n\
     addi t0, t0, 4\naddi t1, t1, -1\n\
     xor a1, a1, t2\nsw a0, -4(t0)\n\
     bnez t1, loop\n\
     slli a2, a0, 2\nsrai a3, a2, 1\n\
     li a7, 93\necall\n";

/// Ejecuta hasta pararse y devuelve, por ciclo, lo retirado y la vista.
fn record(cpu: &mut dyn Processor) -> Vec<(Vec<rv_core::Retired>, PipelineView)> {
    let mut cycles = Vec::new();
    for _ in 0..100_000 {
        if cpu.halt().is_some() {
            return cycles;
        }
        let retired = cpu.step();
        assert_eq!(cpu.view().cycle, cpu.stats().cycles);
        cycles.push((retired, cpu.view().clone()));
    }
    panic!("{} no se ha parado", cpu.name());
}

fn last_stage(view: &PipelineView) -> Stage {
    view.stages.last().unwrap().stage
}

/// `pc`, `rs1`, `rs2`, resultado, siguiente `pc` y dirección.
type Execution = (
    u32,
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Option<u32>,
);

/// Lo que la etapa de ejecución hace con una instrucción; las cargas no tienen resultado en EX.
fn execution(slot: &Slot) -> Execution {
    let loads = slot.control.is_some_and(|control| control.mem_read);
    (
        slot.pc,
        slot.rs1.map(|operand| operand.value),
        slot.rs2.map(|operand| operand.value),
        slot.result.filter(|_| !loads),
        slot.next_pc,
        slot.address,
    )
}

/// Instrucciones por orden de paso por `stage` (vía 0 antes que la 1 en cada ciclo).
fn through(cycles: &[(Vec<rv_core::Retired>, PipelineView)], stage: Stage) -> Vec<Execution> {
    cycles
        .iter()
        .flat_map(|(_, view)| view.slots())
        .filter(|(seen, _, slot)| *seen == stage && slot.state == SlotState::Active)
        .map(|(_, _, slot)| execution(slot))
        .collect()
}

fn check(name: &str, program: &Program, factory: Factory) {
    let mut reference = SingleCycle::new(program).unwrap();
    let reference_cycles = record(&mut reference);
    let mut cpu = factory(program);
    let cycles = record(cpu.as_mut());

    // EX ve cada instrucción una vez, en orden y con los mismos operandos que el de un ciclo.
    assert_eq!(
        through(&cycles, Stage::Ex),
        through(&reference_cycles, Stage::Single),
        "{name}: lo que pasa por EX no cuadra con el de un ciclo"
    );

    let mut flushed = 0;
    for (retired, view) in &cycles {
        let stage = last_stage(view);
        // Lo que se retira, incluida la `ecall` de salida, que además lleva la parada; no
        // lo que falla ni lo descartado.
        let shown: Vec<_> = view
            .slots()
            .filter(|(seen, _, slot)| {
                *seen == stage
                    && slot.state == SlotState::Active
                    && slot
                        .trap
                        .is_none_or(|trap| matches!(trap, Trap::Exited { .. }))
            })
            .map(|(_, _, slot)| (slot.pc, slot.register_write, slot.data_access))
            .collect();
        let expected: Vec<_> = retired
            .iter()
            .map(|entry| (entry.pc, entry.register_write, entry.data_access))
            .collect();
        assert_eq!(
            shown, expected,
            "{name}: la última etapa no cuadra con lo retirado"
        );
        flushed += view
            .slots()
            .filter(|(seen, _, slot)| {
                matches!(seen, Stage::If | Stage::Id) && slot.state == SlotState::Flushed
            })
            .count() as u64;
    }
    let stats = cpu.stats();
    assert_eq!(stats.retired, reference.stats().retired, "{name}");
    assert_eq!(stats.flushed, flushed, "{name}: vaciados");
    check_trace(name, &cycles, stats.retired);
    assert_eq!(cpu.state(), reference.state(), "{name}");
}

fn stage_index(stage: Stage) -> usize {
    Stage::PIPELINE
        .iter()
        .position(|&seen| seen == stage)
        .unwrap_or(0)
}

/// La tabla del pipeline sigue bien a cada instrucción: cada hueco de cada vista está en
/// exactamente una fila con su misma dirección y estado, una fila avanza como mucho una etapa
/// por ciclo y solo se queda en la misma si estaba esperando, y las filas que acaban en la
/// última etapa sin fallar son justo las retiradas.
fn check_trace(name: &str, cycles: &[(Vec<rv_core::Retired>, PipelineView)], retired: u64) {
    let mut trace = PipelineTrace::new(u64::MAX);
    for (index, (_, view)) in cycles.iter().enumerate() {
        trace.record(index as u64 + 1, view);
    }
    let rows: Vec<_> = trace.rows().collect();
    for (index, (_, view)) in cycles.iter().enumerate() {
        let cycle = index as u64 + 1;
        for (stage, lane, slot) in view.slots() {
            let found: Vec<_> = rows
                .iter()
                .filter_map(|row| {
                    row.cells
                        .iter()
                        .find(|cell| {
                            cell.cycle == cycle
                                && cell.stage == stage
                                && usize::from(cell.lane) == lane
                        })
                        .map(|cell| (row.pc, cell.state))
                })
                .collect();
            assert_eq!(
                found,
                [(slot.pc, slot.state)],
                "{name}: ciclo {cycle}, {stage:?} vía {lane}"
            );
        }
    }
    let mut finished = 0;
    for row in &rows {
        for pair in row.cells.windows(2) {
            let (before, after) = (pair[0], pair[1]);
            assert_eq!(after.cycle, before.cycle + 1, "{name}: fila {}", row.id);
            let advance = stage_index(after.stage) as i64 - stage_index(before.stage) as i64;
            assert!(
                advance == 1 || (advance == 0 && before.state == SlotState::Stalled),
                "{name}: fila {} de {:?} a {:?}",
                row.id,
                before,
                after
            );
            assert_ne!(
                before.state,
                SlotState::Flushed,
                "{name}: vaciada que sigue"
            );
        }
        // Ninguna fila se parte: todas empiezan al buscarse y acaban retiradas, vaciadas, con
        // un fallo o cuando el procesador se para.
        let first = row.cells.first().unwrap();
        let first_stage = cycles[first.cycle as usize - 1].1.stages[0].stage;
        assert_eq!(first.stage, first_stage, "{name}: fila {} partida", row.id);
        let last = row.cells.last().unwrap();
        let final_stage = cycles[last.cycle as usize - 1]
            .1
            .stages
            .last()
            .unwrap()
            .stage;
        if last.stage == final_stage && last.state == SlotState::Active && !last.trap {
            finished += 1;
        } else {
            assert!(
                last.state == SlotState::Flushed
                    || last.trap
                    || last.cycle + 1 >= cycles.len() as u64,
                "{name}: fila {} sin terminar en el ciclo {}",
                row.id,
                last.cycle
            );
        }
    }
    // La `ecall` de salida se retira con la parada puesta: no cuenta como fallo.
    let exited = cycles.last().is_some_and(|(_, view)| {
        view.slots()
            .any(|(_, _, slot)| matches!(slot.trap, Some(Trap::Exited { .. })))
    });
    assert_eq!(
        finished + u64::from(exited),
        retired,
        "{name}: retiradas en la tabla"
    );
}

#[test]
fn the_trace_follows_every_instruction_in_every_model() {
    let models: [(&str, Factory); 2] = [
        ("un ciclo", |program| {
            Box::new(SingleCycle::new(program).unwrap())
        }),
        ("5 etapas sin riesgos", |program| {
            Box::new(FiveStage::new(program, Hazards::Ignore).unwrap())
        }),
    ];
    for source in [FACTORIAL, MIXED] {
        let program = assemble(source).unwrap();
        for (name, factory) in models {
            let mut cpu = factory(&program);
            let cycles = record(cpu.as_mut());
            check_trace(name, &cycles, cpu.stats().retired);
        }
    }
}

#[test]
fn the_view_matches_what_every_model_retires() {
    for source in [FACTORIAL, MIXED] {
        let program = assemble(source).unwrap();
        for (name, factory) in PIPELINED {
            check(name, &program, factory);
        }
    }
}

/// Con los riscv-tests, cada operando que EX enseña (adelantado o leído) es el correcto.
#[test]
fn the_view_matches_on_the_riscv_tests() {
    let layout = Layout {
        text_base: 0x8000_0000,
        data_base: 0x1000_0000,
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/riscv-tests");
    let mut checked = 0;
    for suite in ["rv32ui", "rv32um"] {
        let mut paths: Vec<_> = fs::read_dir(root.join(suite))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "s"))
            .collect();
        paths.sort();
        for path in paths {
            let program = assemble_with(&fs::read_to_string(&path).unwrap(), layout).unwrap();
            for (name, factory) in PIPELINED {
                check(&format!("{name} en {}", path.display()), &program, factory);
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 48);
}

/// Los cables que se dibujan cuentan lo mismo que lo que calcula el modelo: la ALU da el
/// resultado, la dirección o la decisión del salto, y el siguiente `pc` sale de donde dice
/// `pc_source`.
fn check_wires(name: &str, slot: &Slot) -> usize {
    let Some(wires) = Wires::of(slot) else {
        return 0;
    };
    let Some(control) = slot.control else {
        return 0;
    };
    let Some(next_pc) = slot.next_pc else {
        assert_eq!((wires.alu, wires.pc_source), (None, None), "{name}");
        return 0;
    };
    let alu = wires.alu;
    match control.operation {
        Operation::Alu(..) | Operation::Lui | Operation::Auipc => {
            assert_eq!(alu.map(|alu| alu.out), slot.result, "{name} {slot:?}");
        }
        Operation::Load(_) | Operation::Store(_) => {
            assert_eq!(alu.map(|alu| alu.out), slot.address, "{name} {slot:?}");
        }
        Operation::Jalr => {
            assert_eq!(
                alu.map(|alu| alu.out & !1),
                Some(next_pc),
                "{name} {slot:?}"
            );
        }
        Operation::Branch(_) => {
            let alu = alu.unwrap();
            assert!(matches!(alu.function, AluFunction::Compare(_)), "{name}");
            assert!(alu.out <= 1, "{name}");
        }
        Operation::Jal | Operation::Fence | Operation::Ecall | Operation::Ebreak => {
            assert_eq!(alu, None, "{name}");
        }
    }
    let expected = match wires.pc_source.unwrap() {
        PcSource::Sequential => wires.pc_plus_4,
        PcSource::Target => wires.target.unwrap(),
        PcSource::Alu => alu.unwrap().out & !1,
    };
    assert_eq!(expected, next_pc, "{name} {slot:?}");
    if control.write_source == Some(rv_core::WriteSource::Link) {
        assert_eq!(slot.result, Some(wires.pc_plus_4), "{name}");
    }
    1
}

fn check_all_wires(name: &str, program: &Program) -> usize {
    let mut checked = 0;
    let models: [(&str, Factory); 4] = [
        ("un ciclo", |program| {
            Box::new(SingleCycle::new(program).unwrap())
        }),
        PIPELINED[0],
        PIPELINED[1],
        PIPELINED[2],
    ];
    for (model, factory) in models {
        let mut cpu = factory(program);
        for (_, view) in record(cpu.as_mut()) {
            for (_, _, slot) in view.slots() {
                checked += check_wires(&format!("{model} en {name}"), slot);
            }
        }
    }
    checked
}

#[test]
fn the_wires_agree_with_what_every_model_computes() {
    for (name, source) in [("factorial", FACTORIAL), ("mixto", MIXED)] {
        assert!(check_all_wires(name, &assemble(source).unwrap()) > 0);
    }
    let layout = Layout {
        text_base: 0x8000_0000,
        data_base: 0x1000_0000,
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/riscv-tests");
    for suite in ["rv32ui", "rv32um"] {
        for entry in fs::read_dir(root.join(suite)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "s") {
                let program = assemble_with(&fs::read_to_string(&path).unwrap(), layout).unwrap();
                check_all_wires(&path.display().to_string(), &program);
            }
        }
    }
}

#[test]
fn stalls_in_five_stages_are_the_cycles_with_id_held() {
    let program = assemble(MIXED).unwrap();
    for hazards in [Hazards::Stall, Hazards::Forward] {
        let mut cpu = FiveStage::new(&program, hazards).unwrap();
        let cycles = record(&mut cpu);
        let held = cycles
            .iter()
            .filter(|(_, view)| {
                view.stage(Stage::Id)
                    .and_then(|id| id.lanes[0])
                    .is_some_and(|slot| slot.state == SlotState::Stalled)
            })
            .count() as u64;
        assert!(held > 0);
        assert_eq!(cpu.stats().stalls, held);
    }
}

#[test]
fn forwarding_shows_where_each_operand_came_from() {
    let program = assemble("li a0, 5\naddi a1, a0, 1\nadd a2, a0, a1\nli a7, 10\necall\n").unwrap();
    let mut cpu = FiveStage::new(&program, Hazards::Forward).unwrap();
    let cycles = record(&mut cpu);
    let operands: Vec<_> = cycles
        .iter()
        .filter_map(|(_, view)| view.stage(Stage::Ex).and_then(|ex| ex.lanes[0]))
        .filter(|slot| slot.pc == 4 || slot.pc == 8)
        .map(|slot| {
            (
                slot.rs1.map(|operand| (operand.value, operand.bypass)),
                slot.rs2.map(|operand| (operand.value, operand.bypass)),
            )
        })
        .collect();
    use rv_core::Bypass::{ExMem, MemWb};
    assert_eq!(
        operands,
        [
            (Some((5, Some(ExMem))), None),
            (Some((5, Some(MemWb))), Some((6, Some(ExMem)))),
        ]
    );
}

/// Lo que produce la instrucción de `lane` en `stage` para adelantar: el resultado de la ALU en
/// MEM (EX/MEM) o lo que escribe en WB (MEM/WB).
fn produced(view: &PipelineView, stage: Stage, lane: u8) -> Option<(rv_isa::Reg, u32)> {
    let slot = view
        .stage(stage)?
        .lanes
        .get(usize::from(lane))
        .copied()
        .flatten()?;
    let reg = slot.instruction?.rd;
    match stage {
        Stage::Mem => slot.result.map(|value| (reg, value)),
        _ => slot.register_write.map(|write| (write.reg, write.value)),
    }
}

#[test]
fn every_bypass_names_the_lane_that_produced_the_value() {
    use rv_core::Bypass::{ExMem, MemWb};
    for source in [FACTORIAL, MIXED] {
        let program = assemble(source).unwrap();
        for (name, factory) in PIPELINED {
            let mut cpu = factory(&program);
            let lanes = u8::try_from(cpu.view().stages[0].lanes.len()).unwrap();
            let mut bypassed = 0;
            for (_, view) in record(cpu.as_mut()) {
                let Some(ex) = view.stage(Stage::Ex) else {
                    continue;
                };
                for slot in ex.lanes.iter().flatten() {
                    for operand in [slot.rs1, slot.rs2].into_iter().flatten() {
                        assert_eq!(operand.bypass.is_some(), operand.bypass_lane.is_some());
                        let (Some(bypass), Some(lane)) = (operand.bypass, operand.bypass_lane)
                        else {
                            continue;
                        };
                        assert!(lane < lanes, "{name}: vía {lane}");
                        let stage = match bypass {
                            ExMem => Stage::Mem,
                            MemWb => Stage::Wb,
                        };
                        assert_eq!(
                            produced(&view, stage, lane),
                            Some((operand.reg, operand.value)),
                            "{name}: pc {:#x}",
                            slot.pc
                        );
                        bypassed += 1;
                    }
                }
            }
            let forwards = name != "5 etapas con burbujas";
            assert_eq!(bypassed > 0, forwards, "{name}");
        }
    }
}

#[test]
fn the_dual_issue_view_has_two_lanes_and_counts_pairs() {
    let program = assemble(MIXED).unwrap();
    let mut cpu = DualIssue::new(&program).unwrap();
    let cycles = record(&mut cpu);
    let pairs = cycles
        .iter()
        .filter(|(_, view)| {
            let ex = view.stage(Stage::Ex).unwrap();
            ex.lanes.len() == 2 && ex.lanes.iter().all(Option::is_some)
        })
        .count() as u64;
    assert!(pairs > 0);
    assert_eq!(cpu.stats().dual_issues, pairs);
    let stats = cpu.stats();
    assert_eq!(
        stats.cpi(),
        Some(stats.cycles as f64 / stats.retired as f64)
    );
    assert!(stats.cpi().unwrap() < 1.5);
}

#[test]
fn the_same_program_gives_the_same_views() {
    let program = assemble(FACTORIAL).unwrap();
    for (name, factory) in PIPELINED {
        let first = record(factory(&program).as_mut());
        let second = record(factory(&program).as_mut());
        assert_eq!(first, second, "{name}");
    }
}

#[test]
fn a_fault_leaves_only_the_faulting_instruction_in_view() {
    let program = assemble("li a0, 1\nsw a0, 0(zero)\nli a7, 10\necall\n").unwrap();
    for (name, factory) in PIPELINED {
        let mut cpu = factory(&program);
        record(cpu.as_mut());
        let halt = cpu.halt().unwrap();
        let shown: Vec<_> = cpu.view().slots().collect();
        assert_eq!(shown.len(), 1, "{name}");
        let (stage, _, slot) = shown[0];
        assert_eq!(stage, Stage::Wb, "{name}");
        assert_eq!((slot.pc, slot.trap), (4, Some(halt.trap)), "{name}");
    }
    let mut cpu = SingleCycle::new(&program).unwrap();
    record(&mut cpu);
    let slot = cpu.view().stages[0].lanes[0].unwrap();
    assert_eq!(slot.pc, 4);
    assert!(slot.trap.is_some() && slot.instruction.is_some());
}
