//! Cachés en la sesión: se configuran desde la interfaz, sobreviven al paso atrás con las
//! mismas cuentas que una ejecución nueva y su vista dice qué línea hay en cada vía.

#![allow(clippy::unwrap_used)]

use rv_core::{CacheConfigError, CacheOp, Model};
use rv_wasm::session::{CacheKind, CacheSettings, Session, SessionError};

const FACTORIAL: &str = include_str!("../../../examples/factorial.s");

/// Veinte pasadas de lectura y escritura sobre un vector más grande que la caché de datos:
/// unos 4000 ciclos, así que el paso atrás cruza varias copias guardadas.
const LONG_SWEEP: &str = "
        .data
v:      .space 256
        .text
        li t2, 20
again:  la t0, v
        li t1, 64
loop:   lw a0, 0(t0)
        addi a0, a0, 1
        sw a0, 0(t0)
        addi t0, t0, 4
        addi t1, t1, -1
        bnez t1, loop
        addi t2, t2, -1
        bnez t2, again
        li a7, 10
        ecall
";

/// Recorre dos veces un vector de 32 palabras: la segunda vuelta acierta toda.
const SWEEP: &str = "
        .data
v:      .space 128
        .text
        li t2, 2
again:  la t0, v
        li t1, 32
loop:   lw a0, 0(t0)
        addi t0, t0, 4
        addi t1, t1, -1
        bnez t1, loop
        addi t2, t2, -1
        bnez t2, again
        li a7, 10
        ecall
";

fn settings(sets: u32, ways: u32, line_bytes: u32, replacement: &str) -> CacheSettings {
    CacheSettings {
        sets,
        ways,
        line_bytes,
        replacement: replacement.to_owned(),
        write_policy: "write-back".to_owned(),
        write_allocate: true,
    }
}

fn session(source: &str, model: Model) -> Session {
    let mut session = Session::new();
    session.set_model(model.id()).unwrap();
    session
        .set_cache(CacheKind::Instruction, Some(settings(4, 1, 16, "lru")))
        .unwrap();
    session
        .set_cache(CacheKind::Data, Some(settings(4, 2, 16, "lru")))
        .unwrap();
    assert!(session.assemble(source).unwrap().ok);
    session
}

#[test]
fn without_configuration_there_are_no_caches() {
    let mut session = Session::new();
    assert!(session.assemble(FACTORIAL).unwrap().ok);
    session.run(1_000_000).unwrap();
    let view = session.caches().unwrap();
    assert!(view.instruction.is_none() && view.data.is_none());
}

#[test]
fn rejects_bad_settings_without_touching_the_current_ones() {
    let mut session = session(SWEEP, Model::SingleCycle);
    assert_eq!(
        session.set_cache(CacheKind::Data, Some(settings(3, 1, 16, "lru"))),
        Err(SessionError::Cache(CacheConfigError::NotPowerOfTwo {
            field: "conjuntos",
            value: 3
        }))
    );
    assert_eq!(
        session.set_cache(CacheKind::Data, Some(settings(4, 1, 16, "plru"))),
        Err(SessionError::UnknownCacheOption("plru".to_owned()))
    );
    let view = session.caches().unwrap();
    assert_eq!(view.data.unwrap().settings, settings(4, 2, 16, "lru"));
}

#[test]
fn second_sweep_hits_and_the_view_shows_lines_and_ages() {
    let mut session = session(SWEEP, Model::FiveStageForwarding);
    session.run(1_000_000).unwrap();
    let data = session.caches().unwrap().data.unwrap();
    // 128 bytes = 8 líneas en 4 conjuntos de 2 vías: caben enteras.
    assert_eq!(data.size_bytes, 128);
    assert_eq!(
        (data.tag_bits, data.index_bits, data.offset_bits),
        (26, 2, 4)
    );
    assert_eq!(data.stats.reads, 64);
    assert_eq!(data.stats.read_hits, 64 - 8);
    assert_eq!(data.stats.evictions, 0);
    assert_eq!(data.history.last().unwrap().accesses, 64);
    for (index, set) in data.sets.iter().enumerate() {
        assert_eq!(set.len(), 2);
        let mut ages: Vec<u32> = set.iter().map(|line| line.age.unwrap()).collect();
        ages.sort_unstable();
        assert_eq!(ages, [0, 1]);
        for line in set {
            let address = line.address.unwrap();
            assert_eq!((address >> 4) & 3, index as u32);
            assert_eq!(address % 16, 0);
        }
    }
}

#[test]
fn step_back_restores_the_same_counts_as_a_fresh_run() {
    for model in Model::ALL.into_iter().filter(|model| model.is_equivalent()) {
        let mut session = session(LONG_SWEEP, model);
        for _ in 0..600 {
            session.step().unwrap();
        }
        for _ in 0..250 {
            session.step_back().unwrap();
        }
        let mut fresh = self::session(LONG_SWEEP, model);
        for _ in 0..350 {
            fresh.step().unwrap();
        }
        assert_eq!(
            session.caches().unwrap(),
            fresh.caches().unwrap(),
            "{model:?}"
        );
    }
}

#[test]
fn recent_accesses_are_the_last_step_only() {
    let mut session = session(SWEEP, Model::SingleCycle);
    for _ in 0..3 {
        session.step().unwrap();
    }
    let view = session.caches().unwrap();
    let recent = view.instruction.unwrap().recent;
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].op, CacheOp::Read);
    // Con la instrucción `lw` en curso, la de datos también enseña su lectura.
    for _ in 0..2 {
        session.step().unwrap();
    }
    let data = session.caches().unwrap().data.unwrap();
    assert_eq!(data.recent.len(), 1);
    assert!(!data.recent[0].hit);
}

#[test]
fn changing_a_cache_restarts_the_program_and_keeps_the_other() {
    let mut session = session(SWEEP, Model::SingleCycle);
    session.run(1_000_000).unwrap();
    session.set_cache(CacheKind::Data, None).unwrap();
    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.cycle, 0);
    let view = session.caches().unwrap();
    assert!(view.data.is_none());
    assert_eq!(view.instruction.unwrap().stats.accesses(), 0);
}

#[test]
fn options_describe_the_limits_and_a_valid_default() {
    let options = Session::cache_options();
    assert_eq!(options.max_sets, 1024);
    assert_eq!(options.replacements.len(), 3);
    assert_eq!(options.write_policies.len(), 2);
    let mut session = Session::new();
    session
        .set_cache(CacheKind::Instruction, Some(options.default))
        .unwrap();
}
