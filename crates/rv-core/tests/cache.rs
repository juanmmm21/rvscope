//! Las cachés observan lo retirado: cuentan lo que el programa lee y escribe, no dependen del
//! modelo de procesador y la E/S no pasa por ellas (ADR 0004).

#![allow(clippy::unwrap_used)]

use rv_core::{CacheConfig, CacheSystem, Model, Processor, Replacement, WritePolicy};

/// Suma un vector de 64 palabras, lo copia a otro y escribe el total en la E/S.
const PROGRAM: &str = "
.data
src: .space 256
dst: .space 256
.text
    la t0, src
    li t1, 64
    li t2, 0
fill:
    sw t2, 0(t0)
    addi t2, t2, 3
    addi t0, t0, 4
    addi t1, t1, -1
    bnez t1, fill
    la t0, src
    la t3, dst
    li t1, 64
    li a0, 0
copy:
    lw t2, 0(t0)
    add a0, a0, t2
    sw t2, 0(t3)
    addi t0, t0, 4
    addi t3, t3, 4
    addi t1, t1, -1
    bnez t1, copy
    li t4, 0xFFFF0000
    sw a0, 0(t4)
    li a7, 10
    ecall
";

fn run(model: Model, caches: &mut CacheSystem) -> (u64, u64, u64) {
    let program = rv_asm::assemble(PROGRAM).unwrap();
    let mut machine = model.boot(&program).unwrap();
    let (mut retired, mut loads, mut stores) = (0, 0, 0);
    for _ in 0..100_000 {
        if machine.halt().is_some() {
            break;
        }
        let cycle = machine.step();
        caches.observe(&cycle);
        retired += cycle.len() as u64;
        for instruction in &cycle {
            match instruction.data_access.map(|access| access.kind) {
                Some(rv_core::AccessKind::Load) => loads += 1,
                Some(rv_core::AccessKind::Store) => stores += 1,
                _ => {}
            }
        }
    }
    assert!(machine.halt().is_some(), "{model:?} no terminó");
    (retired, loads, stores)
}

fn system() -> CacheSystem {
    CacheSystem::new(
        Some(CacheConfig::new(
            4,
            1,
            16,
            Replacement::Lru,
            WritePolicy::WriteBack,
        )),
        Some(CacheConfig::default()),
    )
    .unwrap()
}

#[test]
fn counts_every_fetch_load_and_store_and_skips_io() {
    let mut caches = system();
    let (retired, loads, stores) = run(Model::SingleCycle, &mut caches);
    let instruction = caches.instruction.as_ref().unwrap().stats();
    let data = caches.data.as_ref().unwrap().stats();
    assert_eq!(instruction.reads, retired);
    assert_eq!(instruction.writes, 0);
    assert_eq!(data.reads, loads);
    assert_eq!(data.writes + data.uncached, stores);
    assert_eq!(data.uncached, 1);
    // Los bucles caben en la caché de instrucciones: casi todo acierta.
    assert!(instruction.hit_rate().unwrap() > 0.95);
    // 512 bytes caben justos en la caché de datos: llenar `src` falla una vez por línea (16),
    // al copiar `src` ya está dentro y solo falla `dst` (otras 16), sin expulsar nada.
    assert_eq!(data.misses(), 32);
    assert_eq!(data.evictions, 0);
}

#[test]
fn every_equivalent_model_sees_the_same_accesses() {
    let mut reference = system();
    run(Model::SingleCycle, &mut reference);
    for model in Model::ALL.into_iter().filter(|model| model.is_equivalent()) {
        let mut caches = system();
        run(model, &mut caches);
        assert_eq!(
            caches.instruction.as_ref().unwrap().stats(),
            reference.instruction.as_ref().unwrap().stats(),
            "{model:?}"
        );
        assert_eq!(
            caches.data.as_ref().unwrap().history(),
            reference.data.as_ref().unwrap().history(),
            "{model:?}"
        );
    }
}

#[test]
fn reset_and_rerun_give_the_same_contents() {
    let mut caches = system();
    run(Model::FiveStageForwarding, &mut caches);
    let first = caches.clone();
    caches.reset();
    assert_eq!(caches.data.as_ref().unwrap().stats().accesses(), 0);
    run(Model::FiveStageForwarding, &mut caches);
    assert_eq!(caches, first);
}

#[test]
fn recent_holds_only_the_last_cycle() {
    let program = rv_asm::assemble("li t0, 1\nli t1, 2\nli a7, 10\necall\n").unwrap();
    let mut machine = Model::DualIssue.boot(&program).unwrap();
    let mut caches = CacheSystem::new(Some(CacheConfig::default()), None).unwrap();
    let mut seen = 0;
    for _ in 0..20 {
        let cycle = machine.step();
        caches.observe(&cycle);
        let recent = caches.instruction.as_ref().unwrap().recent();
        assert_eq!(recent.len(), cycle.len());
        seen += recent.len();
    }
    assert_eq!(seen as u64, machine.stats().retired);
}
