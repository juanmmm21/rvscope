//! La sesión que usa la web: ensamblar, paso a paso, paso atrás exacto en todos los modelos,
//! entrada de consola que sobrevive al paso atrás, puntos de parada y lectura de memoria.

#![allow(clippy::unwrap_used)]

use rv_core::{Bypass, Model, SlotState, Stage};
use rv_wasm::session::{
    HaltKind, PipelineTable, RunStop, Session, SessionError, Snapshot, Status, StepOutcome,
    TRACE_WINDOW,
};

const FACTORIAL: &str = include_str!("../../../examples/factorial.s");

/// Lee dos enteros, imprime su suma y una cadena leída, y sale.
const ECHO: &str = "
        .data
buf:    .space 16
        .text
        li a7, 5
        ecall
        mv t0, a0
        li a7, 5
        ecall
        add a0, a0, t0
        li a7, 1
        ecall
        la a0, buf
        li a1, 16
        li a7, 8
        ecall
        la a0, buf
        li a7, 4
        ecall
        li a7, 10
        ecall
";

/// Bucle largo: obliga a guardar muchas copias y a aclararlas.
const LONG_LOOP: &str = "
        li t0, 30000
loop:   addi t1, t1, 3
        addi t0, t0, -1
        bnez t0, loop
        li a7, 10
        ecall
";

fn session(source: &str, model: Model) -> Session {
    let mut session = Session::new();
    session.set_model(model.id()).unwrap();
    assert!(session.assemble(source).unwrap().ok);
    session
}

fn run_to_end(session: &mut Session) -> RunStop {
    session.run(10_000_000).unwrap()
}

#[test]
fn nothing_works_before_assembling() {
    let mut session = Session::new();
    assert_eq!(session.step(), Err(SessionError::NotAssembled));
    assert_eq!(session.step_back(), Err(SessionError::NotAssembled));
    assert_eq!(session.reset(), Err(SessionError::NotAssembled));
    assert!(session.snapshot().is_err());
    // Elegir modelo sin programa sí se puede: es lo que hace la interfaz al arrancar.
    session.set_model("2-vias").unwrap();
    assert_eq!(session.model(), Model::DualIssue);
    assert_eq!(
        session.set_model("cuántico"),
        Err(SessionError::UnknownModel("cuántico".to_owned()))
    );
}

#[test]
fn errors_come_with_position_and_keep_the_previous_program() {
    let mut session = session(FACTORIAL, Model::SingleCycle);
    session.step().unwrap();
    let report = session
        .assemble("nop\n  addi a0, a9, 1\nfoo bar\n")
        .unwrap();
    assert!(!report.ok);
    assert!(report.listing.is_empty());
    assert_eq!(report.errors.len(), 2);
    assert_eq!(report.errors[0].line, 2);
    assert_eq!(report.errors[0].column, 12);
    assert_eq!(report.errors[1].line, 3);
    // El programa anterior sigue cargado donde estaba.
    assert_eq!(session.snapshot().unwrap().stats.cycles, 1);
}

#[test]
fn listing_and_labels_come_with_the_report() {
    let mut session = Session::new();
    let report = session.assemble(FACTORIAL).unwrap();
    assert!(report.ok);
    let first = &report.listing[0];
    assert_eq!(first.address, 0);
    assert_eq!(first.line, 7);
    assert!(first.text.starts_with("auipc a0"));
    assert_eq!(first.pseudo, Some("lw"));
    let factorial = report
        .labels
        .iter()
        .find(|label| label.name == "factorial")
        .unwrap();
    assert!(
        report
            .listing
            .iter()
            .any(|line| line.address == factorial.address
                && line.text.starts_with("addi sp, sp, -8"))
    );
}

#[test]
fn factorial_runs_on_every_equivalent_model() {
    for model in Model::ALL.into_iter().filter(|model| model.is_equivalent()) {
        let mut session = session(FACTORIAL, model);
        assert_eq!(run_to_end(&mut session), RunStop::Halted);
        let snapshot = session.snapshot().unwrap();
        assert_eq!(snapshot.status, Status::Halted);
        assert_eq!(snapshot.registers[10], 720, "{model:?}");
        assert_eq!(snapshot.stats.retired, 85);
        let halt = snapshot.halt.unwrap();
        assert_eq!(halt.kind, HaltKind::Exited);
        assert_eq!(halt.code, Some(0));
        assert_eq!(halt.line, Some(10));
        assert_eq!(session.step().unwrap(), StepOutcome::Halted);
    }
}

#[test]
fn snapshot_marks_changed_registers_and_disassembles_stages() {
    let mut session = session(
        "li a0, 5\nli a1, 7\nadd a2, a0, a1\n",
        Model::FiveStageForwarding,
    );
    let start = session.snapshot().unwrap();
    assert_eq!(start.stages.len(), 5);
    assert!(start.changed.is_empty());
    assert!(!start.can_step_back);
    for _ in 0..5 {
        session.step().unwrap();
    }
    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.changed, vec![10]);
    let wb = snapshot.stages.last().unwrap();
    assert_eq!(wb.name, "WB");
    let retiring = wb.lanes[0].as_ref().unwrap();
    assert_eq!(retiring.text.as_deref(), Some("addi a0, zero, 5"));
    assert_eq!(retiring.line, Some(1));
    let ex = &snapshot.stages[2].lanes[0].as_ref().unwrap();
    assert_eq!(ex.text.as_deref(), Some("add a2, a0, a1"));
    assert_eq!(ex.result, Some(12));
    assert!(snapshot.can_step_back);
}

/// Instantánea y tabla del pipeline tras `steps` pasos, sin volver atrás nunca.
fn fresh(source: &str, model: Model, steps: u64) -> (Snapshot, PipelineTable) {
    let mut session = session(source, model);
    for _ in 0..steps {
        session.step().unwrap();
    }
    state(&session)
}

fn state(session: &Session) -> (Snapshot, PipelineTable) {
    (
        session.snapshot().unwrap(),
        session.pipeline_table().unwrap(),
    )
}

#[test]
fn stepping_back_retraces_every_cycle_on_every_model() {
    for model in Model::ALL {
        let mut session = session(FACTORIAL, model);
        let mut history = vec![state(&session)];
        while session.step().unwrap() == StepOutcome::Stepped {
            history.push(state(&session));
        }
        while let Some(expected) = history.pop() {
            assert_eq!(state(&session), expected, "{model:?}");
            let moved = session.step_back().unwrap();
            assert_eq!(moved, !history.is_empty());
        }
        assert!(!session.snapshot().unwrap().can_step_back);
    }
}

#[test]
fn stepping_back_from_far_away_matches_a_fresh_run() {
    for model in [Model::SingleCycle, Model::DualIssue] {
        let mut session = session(LONG_LOOP, model);
        assert_eq!(run_to_end(&mut session), RunStop::Halted);
        let total = session.snapshot().unwrap().stats.cycles;
        assert!(total > 60_000, "{total}");
        // Ida y vuelta por zonas con copias aclaradas, sin copia cercana y al principio.
        for _ in 0..3 {
            session.step_back().unwrap();
        }
        let at = state(&session);
        assert_eq!(at, fresh(LONG_LOOP, model, at.0.stats.cycles));
        assert_eq!(run_to_end(&mut session), RunStop::Halted);
        assert_eq!(session.snapshot().unwrap().stats.cycles, total);
    }
}

#[test]
fn step_back_after_a_long_run_stays_cheap_and_exact() {
    let mut session = session(LONG_LOOP, Model::FiveStageStalls);
    assert_eq!(session.run(12_345).unwrap(), RunStop::Budget);
    session.step_back().unwrap();
    assert_eq!(
        state(&session),
        fresh(LONG_LOOP, Model::FiveStageStalls, 12_344)
    );
}

#[test]
fn console_input_survives_stepping_back() {
    for model in Model::ALL.into_iter().filter(|model| model.is_equivalent()) {
        let mut session = session(ECHO, model);
        assert_eq!(run_to_end(&mut session), RunStop::AwaitingInput);
        assert_eq!(session.snapshot().unwrap().status, Status::AwaitingInput);
        // Esperar más no cambia nada ni se apunta en la historia.
        let waiting = session.snapshot().unwrap();
        assert_eq!(session.step().unwrap(), StepOutcome::AwaitingInput);
        assert_eq!(session.snapshot().unwrap(), waiting);

        session.push_input("40\n2\n").unwrap();
        assert_eq!(run_to_end(&mut session), RunStop::AwaitingInput);
        assert_eq!(session.snapshot().unwrap().output, "42");
        session.push_input("hola\n").unwrap();
        assert_eq!(run_to_end(&mut session), RunStop::Halted);
        let end = session.snapshot().unwrap();
        let end_table = session.pipeline_table().unwrap();
        assert_eq!(end.output, "42hola\n");

        // Volver hasta antes de la primera lectura devuelve toda la entrada a la cola.
        while session.snapshot().unwrap().stats.retired > 1 {
            session.step_back().unwrap();
        }
        let rewound = session.snapshot().unwrap();
        assert_eq!(rewound.output, "");
        assert_eq!(rewound.pending_input, "40\n2\nhola\n".len());
        assert_eq!(run_to_end(&mut session), RunStop::Halted);
        assert_eq!(session.snapshot().unwrap(), end, "{model:?}");
        assert_eq!(session.pipeline_table().unwrap(), end_table, "{model:?}");
    }
}

#[test]
fn the_pipeline_table_shows_the_last_cycles_with_stalls_and_bypasses() {
    let mut session = session(FACTORIAL, Model::FiveStageForwarding);
    let empty = session.pipeline_table().unwrap();
    assert_eq!((empty.first_cycle, empty.last_cycle), (0, 0));
    assert!(empty.rows.is_empty());

    assert_eq!(session.run(60).unwrap(), RunStop::Budget);
    let table = session.pipeline_table().unwrap();
    assert_eq!(table.last_cycle, 60);
    assert_eq!(table.first_cycle, 60 - TRACE_WINDOW + 1);
    let cells = || table.rows.iter().flat_map(|row| &row.cells);
    assert!(cells().all(|cell| cell.cycle >= table.first_cycle && cell.cycle <= 60));
    // Cada fila lleva su desensamblado y su línea, y en el último ciclo hay una por etapa.
    assert!(
        table
            .rows
            .iter()
            .all(|row| row.text.is_some() && row.line.is_some())
    );
    let last: Vec<Stage> = cells()
        .filter(|cell| cell.cycle == 60)
        .map(|cell| cell.stage)
        .collect();
    assert!(last.len() <= 5);
    // A lo largo del factorial hay cargas usadas enseguida (carga-uso) y resultados
    // adelantados a EX desde los dos registros de segmentación, siempre en EX.
    let mut stalls = 0;
    let mut bypassed = Vec::new();
    while session.step().unwrap() == StepOutcome::Stepped {
        let table = session.pipeline_table().unwrap();
        let cycle = table.last_cycle;
        for cell in table
            .rows
            .iter()
            .flat_map(|row| &row.cells)
            .filter(|cell| cell.cycle == cycle)
        {
            if cell.state == SlotState::Stalled {
                stalls += 1;
            }
            for bypass in cell.bypasses.iter().flatten() {
                assert_eq!(cell.stage, Stage::Ex);
                bypassed.push(bypass.from);
            }
        }
    }
    assert!(stalls > 0);
    assert!(bypassed.contains(&Bypass::ExMem) && bypassed.contains(&Bypass::MemWb));

    session.reset().unwrap();
    assert!(session.pipeline_table().unwrap().rows.is_empty());
}

#[test]
fn closed_input_ends_the_read_with_an_error() {
    let mut session = session(ECHO, Model::FiveStageForwarding);
    run_to_end(&mut session);
    session.close_input().unwrap();
    assert_eq!(run_to_end(&mut session), RunStop::Halted);
    let snapshot = session.snapshot().unwrap();
    assert!(snapshot.input_closed);
    let halt = snapshot.halt.unwrap();
    assert_eq!(halt.kind, HaltKind::Fault);
    assert_eq!(halt.line, Some(6));
    assert_eq!(halt.message, "ecall: no queda entrada que leer");
}

#[test]
fn breakpoints_stop_before_the_instruction_and_resume() {
    for model in Model::ALL.into_iter().filter(|model| model.is_equivalent()) {
        let mut session = session(FACTORIAL, model);
        let report = session.assemble(FACTORIAL).unwrap();
        let entry = report
            .labels
            .iter()
            .find(|label| label.name == "factorial")
            .unwrap()
            .address;
        session.set_breakpoints(&[entry, entry]);
        // Seis llamadas recursivas: seis paradas y luego el final.
        for _ in 0..6 {
            assert_eq!(
                run_to_end(&mut session),
                RunStop::Breakpoint { pc: entry },
                "{model:?}"
            );
        }
        assert_eq!(run_to_end(&mut session), RunStop::Halted, "{model:?}");
        assert_eq!(session.snapshot().unwrap().registers[10], 720);
    }
}

#[test]
fn single_cycle_breakpoint_leaves_pc_on_the_instruction() {
    let mut session = session("nop\nnop\nnop\nli a7, 10\necall\n", Model::SingleCycle);
    session.set_breakpoints(&[8]);
    assert_eq!(run_to_end(&mut session), RunStop::Breakpoint { pc: 8 });
    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.pc, 8);
    assert_eq!(snapshot.stats.retired, 2);
}

#[test]
fn run_respects_the_cycle_budget() {
    let mut session = session("loop: j loop\n", Model::FiveStageForwarding);
    assert_eq!(session.run(1000).unwrap(), RunStop::Budget);
    assert_eq!(session.snapshot().unwrap().stats.cycles, 1000);
}

#[test]
fn changing_model_or_resetting_starts_over() {
    let mut session = session(FACTORIAL, Model::SingleCycle);
    session.run(10).unwrap();
    session.set_model("5-etapas-burbujas").unwrap();
    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.model, "5-etapas-burbujas");
    assert_eq!(snapshot.stats.cycles, 0);
    assert_eq!(snapshot.stages.len(), 5);
    session.run(10).unwrap();
    session.reset().unwrap();
    assert_eq!(session.snapshot().unwrap().stats.cycles, 0);
}

#[test]
fn memory_is_read_by_ranges() {
    let session = session(FACTORIAL, Model::SingleCycle);
    let data = session.read_memory(0x1000_0000, 4).unwrap();
    assert_eq!(data, vec![6, 0, 0, 0]);
    assert!(matches!(
        session.read_memory(0, 1 << 20),
        Err(SessionError::ReadTooLarge(_))
    ));
    let regions = session.regions().unwrap();
    assert_eq!(regions.len(), 4);
}

#[test]
fn every_model_is_listed() {
    let models = Session::models();
    assert_eq!(models.len(), Model::ALL.len());
    assert_eq!(models[0].id, "un-ciclo");
    assert!(!models[1].equivalent);
    let mut labels: Vec<&str> = models.iter().map(|model| model.label).collect();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(labels.len(), models.len());
}

#[test]
fn changing_model_keeps_program_and_breakpoints() {
    let mut session = session(FACTORIAL, Model::SingleCycle);
    // `call factorial` está en 0x08: el primer punto de parada tras la entrada.
    session.set_breakpoints(&[8]);
    for model in Model::ALL.into_iter().filter(|model| model.is_equivalent()) {
        session.set_model(model.id()).unwrap();
        assert_eq!(session.run(10_000).unwrap(), RunStop::Breakpoint { pc: 8 });
        session.set_breakpoints(&[]);
        assert_eq!(session.run(10_000).unwrap(), RunStop::Halted);
        assert_eq!(session.snapshot().unwrap().registers[10], 720);
        session.set_breakpoints(&[8]);
    }
}

#[test]
fn vocabulary_lists_every_word_the_assembler_knows() {
    let vocabulary = Session::vocabulary();
    assert_eq!(vocabulary.instructions.len(), rv_isa::INSTRUCTIONS.len());
    assert!(vocabulary.instructions.contains(&"addi"));
    assert!(vocabulary.pseudo_instructions.contains(&"li"));
    assert!(vocabulary.directives.contains(&".word"));
    assert_eq!(vocabulary.modifiers, ["hi", "lo"]);
    // 32 nombres ABI, 32 numéricos y `fp`; todos los entiende el ensamblador.
    assert_eq!(vocabulary.registers.len(), 65);
    for name in &vocabulary.registers {
        assert!(rv_isa::Reg::parse(name).is_some(), "{name}");
    }
}
