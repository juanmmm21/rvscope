//! Conformidad con los riscv-tests oficiales (rv32ui y rv32um).
//!
//! Los `.s` de `tests/riscv-tests` son los tests del repositorio oficial ya preprocesados por
//! `scripts/vendor-riscv-tests.sh`, con un entorno propio que termina con la llamada al
//! sistema de salida: código 0 si todos los casos pasan y el número del caso que falla si no.
//! Cada modelo de procesador nuevo se añade a `MODELS` y tiene que pasar los mismos tests y
//! acabar en el mismo estado que el de un ciclo.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use rv_asm::{Layout, Program, assemble_with};
use rv_core::{ArchState, FiveStage, Hazards, Processor, RunOutcome, SingleCycle, Trap};

/// El entorno oficial sitúa el código en 0x80000000; `auipc.S` depende de que una dirección
/// 10000 bytes por debajo del código siga siendo válida.
const LAYOUT: Layout = Layout {
    text_base: 0x8000_0000,
    data_base: 0x1000_0000,
};

/// Ningún test pasa de unos pocos miles de ciclos; el tope solo evita colgarse.
const MAX_CYCLES: u64 = 1_000_000;

type Factory = fn(&Program) -> Box<dyn Processor>;

const MODELS: [(&str, Factory); 3] = [
    ("un ciclo", |program| {
        Box::new(SingleCycle::new(program).unwrap())
    }),
    ("5 etapas con burbujas", |program| {
        Box::new(FiveStage::new(program, Hazards::Stall).unwrap())
    }),
    ("5 etapas con forwarding", |program| {
        Box::new(FiveStage::new(program, Hazards::Forward).unwrap())
    }),
];

fn sources(suite: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/riscv-tests")
        .join(suite);
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "s"))
        .collect();
    files.sort();
    files
}

/// Ejecuta un test en un modelo y devuelve su estado final, o por qué falló.
fn run(path: &Path, factory: Factory) -> Result<ArchState, String> {
    let source = fs::read_to_string(path).unwrap();
    let program = assemble_with(&source, LAYOUT).map_err(|errors| {
        let shown: Vec<String> = errors.iter().take(3).map(ToString::to_string).collect();
        format!("no ensambla: {}", shown.join(" | "))
    })?;
    let mut processor = factory(&program);
    match processor.run(MAX_CYCLES) {
        RunOutcome::Halted(halt) => match halt.trap {
            Trap::Exited { code: 0 } => Ok(processor.state().clone()),
            Trap::Exited { code } => Err(format!("falla el caso {code}")),
            other => Err(format!("se para en 0x{:08x}: {other}", halt.pc)),
        },
        RunOutcome::CycleLimit => Err(format!("no termina en {MAX_CYCLES} ciclos")),
        RunOutcome::AwaitingInput => Err("espera entrada".to_owned()),
    }
}

fn check_suite(suite: &str, expected_tests: usize) {
    let files = sources(suite);
    assert_eq!(
        files.len(),
        expected_tests,
        "faltan o sobran tests en {suite}: {files:?}"
    );
    let mut failures = Vec::new();
    for path in &files {
        let name = path.file_stem().unwrap().to_string_lossy();
        let mut reference: Option<ArchState> = None;
        for (model, factory) in MODELS {
            match run(path, factory) {
                Ok(state) => match &reference {
                    None => reference = Some(state),
                    Some(expected) if *expected != state => failures.push(format!(
                        "{suite}/{name} ({model}): estado final distinto del de un ciclo"
                    )),
                    Some(_) => {}
                },
                Err(reason) => failures.push(format!("{suite}/{name} ({model}): {reason}")),
            }
        }
    }
    assert!(failures.is_empty(), "riscv-tests:\n{}", failures.join("\n"));
}

#[test]
fn rv32ui() {
    // Los 42 oficiales menos fence_i (Zifencei) y ma_data (accesos desalineados).
    check_suite("rv32ui", 40);
}

#[test]
fn rv32um() {
    check_suite("rv32um", 8);
}

#[test]
fn a_failing_case_is_reported() {
    // Comprueba el propio entorno: si un caso falla, la salida lleva su número.
    let dir = std::env::temp_dir().join("rvscope-riscv-tests-env");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fail.s");
    fs::write(
        &path,
        "_start: test_2: li gp, 2; li a0, 1; li x7, 2; bne a0, x7, fail\n\
         bne x0, gp, pass\n\
         fail: mv a0, gp; li a7, 93; ecall\n\
         pass: li a0, 0; li a7, 93; ecall\n",
    )
    .unwrap();
    assert_eq!(
        run(&path, MODELS[0].1).unwrap_err(),
        "falla el caso 2".to_owned()
    );
}
