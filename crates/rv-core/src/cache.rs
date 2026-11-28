//! Cachés de instrucciones y de datos que miden aciertos y fallos sin cambiar ni el estado
//! arquitectónico ni los ciclos (ADR 0004). Se alimentan de lo que retira el procesador, así que
//! todos los modelos equivalentes dan las mismas estadísticas.

use serde::Serialize;
use thiserror::Error;

use crate::map::IO_BASE;
use crate::memory::AccessKind;
use crate::processor::Retired;

/// Política de reemplazo cuando el conjunto está lleno.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Replacement {
    Lru,
    Fifo,
    Random,
}

impl Replacement {
    pub const ALL: [Replacement; 3] = [Replacement::Lru, Replacement::Fifo, Replacement::Random];

    pub fn id(self) -> &'static str {
        match self {
            Replacement::Lru => "lru",
            Replacement::Fifo => "fifo",
            Replacement::Random => "aleatoria",
        }
    }

    pub fn from_id(id: &str) -> Option<Replacement> {
        Replacement::ALL
            .into_iter()
            .find(|replacement| replacement.id() == id)
    }
}

/// Qué pasa con un almacenamiento que acierta.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WritePolicy {
    /// La línea queda sucia y se escribe en memoria al expulsarla.
    WriteBack,
    /// Cada almacenamiento se escribe también en memoria.
    WriteThrough,
}

impl WritePolicy {
    pub const ALL: [WritePolicy; 2] = [WritePolicy::WriteBack, WritePolicy::WriteThrough];

    pub fn id(self) -> &'static str {
        match self {
            WritePolicy::WriteBack => "write-back",
            WritePolicy::WriteThrough => "write-through",
        }
    }

    pub fn from_id(id: &str) -> Option<WritePolicy> {
        WritePolicy::ALL
            .into_iter()
            .find(|policy| policy.id() == id)
    }
}

pub const MAX_SETS: u32 = 1024;
pub const MAX_WAYS: u32 = 16;
pub const MIN_LINE_BYTES: u32 = 4;
pub const MAX_LINE_BYTES: u32 = 256;

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum CacheConfigError {
    #[error("{field}: {value} no es una potencia de dos")]
    NotPowerOfTwo { field: &'static str, value: u32 },
    #[error("{field}: {value} está fuera del rango admitido ({min} a {max})")]
    OutOfRange {
        field: &'static str,
        value: u32,
        min: u32,
        max: u32,
    },
}

/// Geometría y políticas de una caché.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct CacheConfig {
    pub sets: u32,
    pub ways: u32,
    pub line_bytes: u32,
    pub replacement: Replacement,
    pub write_policy: WritePolicy,
    /// Si un almacenamiento que falla trae la línea a la caché.
    pub write_allocate: bool,
}

impl Default for CacheConfig {
    /// 16 conjuntos de 2 vías con líneas de 16 bytes (512 bytes), LRU y write-back.
    fn default() -> CacheConfig {
        CacheConfig::new(16, 2, 16, Replacement::Lru, WritePolicy::WriteBack)
    }
}

impl CacheConfig {
    /// Configuración con la asignación en escritura de los libros: con write-back sí, con
    /// write-through no.
    pub fn new(
        sets: u32,
        ways: u32,
        line_bytes: u32,
        replacement: Replacement,
        write_policy: WritePolicy,
    ) -> CacheConfig {
        CacheConfig {
            sets,
            ways,
            line_bytes,
            replacement,
            write_policy,
            write_allocate: write_policy == WritePolicy::WriteBack,
        }
    }

    pub fn validate(&self) -> Result<(), CacheConfigError> {
        check("conjuntos", self.sets, 1, MAX_SETS)?;
        check("vías", self.ways, 1, MAX_WAYS)?;
        check(
            "bytes por línea",
            self.line_bytes,
            MIN_LINE_BYTES,
            MAX_LINE_BYTES,
        )
    }

    /// Capacidad de datos en bytes (sin contar etiquetas ni bits de control).
    pub fn size_bytes(&self) -> u32 {
        self.sets * self.ways * self.line_bytes
    }

    pub fn offset_bits(&self) -> u32 {
        self.line_bytes.trailing_zeros()
    }

    pub fn index_bits(&self) -> u32 {
        self.sets.trailing_zeros()
    }

    /// Parte una dirección en (etiqueta, conjunto, desplazamiento dentro de la línea).
    pub fn locate(&self, address: u32) -> (u32, u32, u32) {
        let offset = address & (self.line_bytes - 1);
        let set = (address >> self.offset_bits()) & (self.sets - 1);
        let tag = address >> (self.offset_bits() + self.index_bits());
        (tag, set, offset)
    }

    /// Dirección del primer byte de la línea con esa etiqueta en ese conjunto.
    pub fn line_address(&self, tag: u32, set: u32) -> u32 {
        (tag << (self.offset_bits() + self.index_bits())) | (set << self.offset_bits())
    }
}

fn check(field: &'static str, value: u32, min: u32, max: u32) -> Result<(), CacheConfigError> {
    if !value.is_power_of_two() {
        return Err(CacheConfigError::NotPowerOfTwo { field, value });
    }
    if value < min || value > max {
        return Err(CacheConfigError::OutOfRange {
            field,
            value,
            min,
            max,
        });
    }
    Ok(())
}

/// Una vía de un conjunto.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Line {
    pub valid: bool,
    pub dirty: bool,
    pub tag: u32,
    /// Número del último acceso que la tocó (para LRU).
    pub last_used: u64,
    /// Número del acceso que la llenó (para FIFO).
    pub filled_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheOp {
    Read,
    Write,
}

/// Línea expulsada por un fallo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Eviction {
    pub tag: u32,
    pub address: u32,
    /// Si estaba sucia y hubo que escribirla en memoria.
    pub dirty: bool,
}

/// Resultado de un acceso, para que la vista resalte la línea tocada.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct CacheAccess {
    pub op: CacheOp,
    pub address: u32,
    pub tag: u32,
    pub set: u32,
    /// Vía donde está ahora la línea; `None` en un fallo de escritura sin asignación.
    pub way: Option<u32>,
    pub hit: bool,
    pub eviction: Option<Eviction>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CacheStats {
    pub reads: u64,
    pub read_hits: u64,
    pub writes: u64,
    pub write_hits: u64,
    /// Líneas válidas sustituidas por otra.
    pub evictions: u64,
    /// Líneas sucias escritas en memoria al expulsarlas (write-back).
    pub writebacks: u64,
    /// Almacenamientos que van directos a memoria: todos en write-through y los fallos sin
    /// asignación en write-back.
    pub memory_writes: u64,
    /// Accesos a la E/S, que no pasan por la caché.
    pub uncached: u64,
}

impl CacheStats {
    pub fn accesses(&self) -> u64 {
        self.reads + self.writes
    }

    pub fn hits(&self) -> u64 {
        self.read_hits + self.write_hits
    }

    pub fn misses(&self) -> u64 {
        self.accesses() - self.hits()
    }

    /// Aciertos entre accesos; `None` si aún no ha habido ninguno.
    pub fn hit_rate(&self) -> Option<f64> {
        (self.accesses() > 0).then(|| self.hits() as f64 / self.accesses() as f64)
    }
}

/// Totales acumulados en un momento de la ejecución.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct HistoryPoint {
    pub accesses: u64,
    pub hits: u64,
}

pub const MAX_HISTORY_POINTS: usize = 256;

/// Evolución de los aciertos para el gráfico: un punto cada `stride` accesos y nunca más de
/// `MAX_HISTORY_POINTS`, para que quepa en cada instantánea del paso atrás aunque el programa
/// haga millones de accesos.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HitHistory {
    pub stride: u64,
    pub points: Vec<HistoryPoint>,
}

impl Default for HitHistory {
    fn default() -> HitHistory {
        HitHistory {
            stride: 1,
            points: Vec::new(),
        }
    }
}

impl HitHistory {
    fn record(&mut self, stats: &CacheStats) {
        let accesses = stats.accesses();
        if accesses % self.stride != 0 {
            return;
        }
        self.points.push(HistoryPoint {
            accesses,
            hits: stats.hits(),
        });
        if self.points.len() > MAX_HISTORY_POINTS {
            // Los puntos están en stride·1, stride·2…; quedarse con los pares deja la misma
            // separación regular con el doble de paso.
            self.points = self.points.iter().skip(1).step_by(2).copied().collect();
            self.stride *= 2;
        }
    }
}

const RANDOM_SEED: u32 = 0x9E37_79B9;

/// Una caché con su contenido, sus contadores y lo que pasó en el último ciclo observado.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Cache {
    config: CacheConfig,
    /// `sets × ways` líneas, conjunto a conjunto.
    lines: Vec<Line>,
    stats: CacheStats,
    history: HitHistory,
    clock: u64,
    rng: u32,
    recent: Vec<CacheAccess>,
}

impl Cache {
    pub fn new(config: CacheConfig) -> Result<Cache, CacheConfigError> {
        config.validate()?;
        Ok(Cache {
            config,
            lines: vec![Line::default(); (config.sets * config.ways) as usize],
            stats: CacheStats::default(),
            history: HitHistory::default(),
            clock: 0,
            rng: RANDOM_SEED,
            recent: Vec::new(),
        })
    }

    pub fn config(&self) -> &CacheConfig {
        &self.config
    }

    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    pub fn history(&self) -> &HitHistory {
        &self.history
    }

    /// Las vías de un conjunto; vacío si el conjunto no existe.
    pub fn set(&self, set: u32) -> &[Line] {
        let ways = self.config.ways as usize;
        let start = set as usize * ways;
        self.lines.get(start..start + ways).unwrap_or(&[])
    }

    /// Accesos de la última llamada a `CacheSystem::observe` (los de un ciclo).
    pub fn recent(&self) -> &[CacheAccess] {
        &self.recent
    }

    /// Vacía la caché y los contadores; con la semilla de nuevo al principio, la misma
    /// ejecución vuelve a dar lo mismo.
    pub fn reset(&mut self) {
        *self = Cache {
            config: self.config,
            lines: vec![Line::default(); self.lines.len()],
            stats: CacheStats::default(),
            history: HitHistory::default(),
            clock: 0,
            rng: RANDOM_SEED,
            recent: Vec::new(),
        };
    }

    pub fn access(&mut self, address: u32, op: CacheOp) -> CacheAccess {
        self.clock += 1;
        let clock = self.clock;
        let config = self.config;
        let (tag, set, _) = config.locate(address);
        let start = set as usize * config.ways as usize;
        let end = start + config.ways as usize;
        let found = self.lines[start..end]
            .iter()
            .position(|line| line.valid && line.tag == tag);

        let (way, eviction) = match found {
            Some(way) => (Some(way), None),
            None if op == CacheOp::Write && !config.write_allocate => (None, None),
            None => {
                let victim = self.victim(start, end);
                let old = self.lines[start + victim];
                let eviction = old.valid.then(|| Eviction {
                    tag: old.tag,
                    address: config.line_address(old.tag, set),
                    dirty: old.dirty,
                });
                self.lines[start + victim] = Line {
                    valid: true,
                    dirty: false,
                    tag,
                    last_used: clock,
                    filled_at: clock,
                };
                (Some(victim), eviction)
            }
        };

        let hit = found.is_some();
        match op {
            CacheOp::Read => {
                self.stats.reads += 1;
                self.stats.read_hits += u64::from(hit);
            }
            CacheOp::Write => {
                self.stats.writes += 1;
                self.stats.write_hits += u64::from(hit);
            }
        }
        if let Some(eviction) = eviction {
            self.stats.evictions += 1;
            self.stats.writebacks += u64::from(eviction.dirty);
        }
        if let Some(way) = way {
            let line = &mut self.lines[start + way];
            line.last_used = clock;
            if op == CacheOp::Write && config.write_policy == WritePolicy::WriteBack {
                line.dirty = true;
            }
        }
        if op == CacheOp::Write
            && (config.write_policy == WritePolicy::WriteThrough || way.is_none())
        {
            self.stats.memory_writes += 1;
        }
        self.history.record(&self.stats);

        let access = CacheAccess {
            op,
            address,
            tag,
            set,
            way: way.map(|way| way as u32),
            hit,
            eviction,
        };
        self.recent.push(access);
        access
    }

    /// Vía que se sustituye en el conjunto `lines[start..end]`: siempre antes una vacía.
    fn victim(&mut self, start: usize, end: usize) -> usize {
        let lines = &self.lines[start..end];
        if let Some(empty) = lines.iter().position(|line| !line.valid) {
            return empty;
        }
        match self.config.replacement {
            Replacement::Lru => oldest(lines, |line| line.last_used),
            Replacement::Fifo => oldest(lines, |line| line.filled_at),
            Replacement::Random => {
                // xorshift32: determinista y con estado en la propia caché, así que el paso
                // atrás y la re-ejecución dan la misma secuencia.
                let mut x = self.rng;
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                self.rng = x;
                (x % self.config.ways) as usize
            }
        }
    }
}

fn oldest(lines: &[Line], stamp: impl Fn(&Line) -> u64) -> usize {
    lines
        .iter()
        .enumerate()
        .min_by_key(|(_, line)| stamp(line))
        .map_or(0, |(way, _)| way)
}

/// Las dos cachés del procesador, cada una opcional.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CacheSystem {
    pub instruction: Option<Cache>,
    pub data: Option<Cache>,
}

impl CacheSystem {
    pub fn new(
        instruction: Option<CacheConfig>,
        data: Option<CacheConfig>,
    ) -> Result<CacheSystem, CacheConfigError> {
        Ok(CacheSystem {
            instruction: instruction.map(Cache::new).transpose()?,
            data: data.map(Cache::new).transpose()?,
        })
    }

    pub fn reset(&mut self) {
        for cache in [&mut self.instruction, &mut self.data]
            .into_iter()
            .flatten()
        {
            cache.reset();
        }
    }

    /// Pasa por las cachés lo retirado en un ciclo: por cada instrucción, su lectura en la de
    /// instrucciones y después su carga o almacenamiento en la de datos.
    pub fn observe(&mut self, retired: &[Retired]) {
        for cache in [&mut self.instruction, &mut self.data]
            .into_iter()
            .flatten()
        {
            cache.recent.clear();
        }
        for instruction in retired {
            if let Some(cache) = &mut self.instruction {
                touch(cache, instruction.pc, CacheOp::Read);
            }
            if let (Some(cache), Some(access)) = (&mut self.data, instruction.data_access) {
                let op = match access.kind {
                    AccessKind::Store => CacheOp::Write,
                    AccessKind::Load | AccessKind::Fetch => CacheOp::Read,
                };
                touch(cache, access.address, op);
            }
        }
    }
}

fn touch(cache: &mut Cache, address: u32, op: CacheOp) {
    if address >= IO_BASE {
        cache.stats.uncached += 1;
    } else {
        cache.access(address, op);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache(sets: u32, ways: u32, line: u32, replacement: Replacement) -> Cache {
        Cache::new(CacheConfig::new(
            sets,
            ways,
            line,
            replacement,
            WritePolicy::WriteBack,
        ))
        .unwrap()
    }

    fn reads(cache: &mut Cache, addresses: &[u32]) -> Vec<bool> {
        addresses
            .iter()
            .map(|&address| cache.access(address, CacheOp::Read).hit)
            .collect()
    }

    #[test]
    fn rejects_impossible_geometries() {
        let base = CacheConfig::default();
        assert_eq!(base.validate(), Ok(()));
        assert_eq!(base.size_bytes(), 512);
        assert_eq!(
            CacheConfig { sets: 12, ..base }.validate(),
            Err(CacheConfigError::NotPowerOfTwo {
                field: "conjuntos",
                value: 12
            })
        );
        assert_eq!(
            CacheConfig { ways: 32, ..base }.validate(),
            Err(CacheConfigError::OutOfRange {
                field: "vías",
                value: 32,
                min: 1,
                max: 16
            })
        );
        assert!(
            CacheConfig {
                line_bytes: 2,
                ..base
            }
            .validate()
            .is_err()
        );
        assert!(CacheConfig { sets: 0, ..base }.validate().is_err());
        assert!(
            Cache::new(CacheConfig {
                line_bytes: 512,
                ..base
            })
            .is_err()
        );
    }

    #[test]
    fn splits_addresses_into_tag_set_and_offset() {
        let config = CacheConfig::new(16, 1, 16, Replacement::Lru, WritePolicy::WriteBack);
        assert_eq!(config.offset_bits(), 4);
        assert_eq!(config.index_bits(), 4);
        assert_eq!(config.locate(0x1234_5678), (0x0012_3456, 0x7, 0x8));
        assert_eq!(config.line_address(0x0012_3456, 0x7), 0x1234_5670);
        let single = CacheConfig::new(1, 4, 4, Replacement::Lru, WritePolicy::WriteBack);
        assert_eq!(single.locate(0xFFFF_FFFC), (0x3FFF_FFFF, 0, 0));
    }

    #[test]
    fn sequential_words_hit_three_of_four_with_sixteen_byte_lines() {
        let mut cache = cache(16, 1, 16, Replacement::Lru);
        let addresses: Vec<u32> = (0..64).map(|i| 0x1000_0000 + 4 * i).collect();
        reads(&mut cache, &addresses);
        assert_eq!(cache.stats().accesses(), 64);
        assert_eq!(cache.stats().hits(), 48);
        assert_eq!(cache.stats().hit_rate(), Some(0.75));
    }

    #[test]
    fn associativity_removes_conflict_misses() {
        // Dos direcciones a 256 bytes caen en el mismo conjunto de una caché de 16 × 16 bytes.
        let pattern = [0x0, 0x100, 0x0, 0x100, 0x0, 0x100];
        let mut direct = cache(16, 1, 16, Replacement::Lru);
        assert_eq!(reads(&mut direct, &pattern), [false; 6]);
        assert_eq!(direct.stats().evictions, 5);
        let mut two_way = cache(8, 2, 16, Replacement::Lru);
        assert_eq!(
            reads(&mut two_way, &pattern),
            [false, false, true, true, true, true]
        );
    }

    #[test]
    fn lru_and_fifo_pick_different_victims() {
        // A B A C A en un conjunto de dos vías: LRU expulsa B y FIFO expulsa A.
        let pattern = [0x0, 0x10, 0x0, 0x20, 0x0];
        let mut lru = cache(1, 2, 16, Replacement::Lru);
        assert_eq!(reads(&mut lru, &pattern), [false, false, true, false, true]);
        let mut fifo = cache(1, 2, 16, Replacement::Fifo);
        assert_eq!(
            reads(&mut fifo, &pattern),
            [false, false, true, false, false]
        );
    }

    #[test]
    fn random_replacement_is_reproducible() {
        let pattern: Vec<u32> = (0..500).map(|i| (i * 37 % 23) * 16).collect();
        let mut first = cache(1, 4, 16, Replacement::Random);
        let hits = reads(&mut first, &pattern);
        let mut second = cache(1, 4, 16, Replacement::Random);
        assert_eq!(reads(&mut second, &pattern), hits);
        first.reset();
        assert_eq!(first.stats(), &CacheStats::default());
        assert_eq!(reads(&mut first, &pattern), hits);
        assert_eq!(first, second);
    }

    #[test]
    fn fills_an_empty_way_before_evicting() {
        let mut cache = cache(1, 4, 16, Replacement::Random);
        reads(&mut cache, &[0x00, 0x10, 0x20, 0x30]);
        assert_eq!(cache.stats().evictions, 0);
        assert!(cache.set(0).iter().all(|line| line.valid));
        assert_eq!(reads(&mut cache, &[0x00, 0x10, 0x20, 0x30]), [true; 4]);
    }

    #[test]
    fn write_back_writes_dirty_lines_only_when_evicted() {
        let mut cache = cache(1, 1, 16, Replacement::Lru);
        let store = cache.access(0x40, CacheOp::Write);
        assert!(!store.hit);
        assert_eq!(store.way, Some(0));
        assert!(cache.set(0)[0].dirty);
        cache.access(0x44, CacheOp::Write);
        assert_eq!(cache.stats().memory_writes, 0);
        let load = cache.access(0x80, CacheOp::Read);
        assert_eq!(
            load.eviction,
            Some(Eviction {
                tag: 0x4,
                address: 0x40,
                dirty: true
            })
        );
        assert_eq!(cache.stats().writebacks, 1);
        assert!(!cache.set(0)[0].dirty);
        cache.access(0xC0, CacheOp::Read);
        assert_eq!(cache.stats().evictions, 2);
        assert_eq!(cache.stats().writebacks, 1);
        assert_eq!(cache.stats().write_hits, 1);
    }

    #[test]
    fn write_through_without_allocation_sends_every_store_to_memory() {
        let config = CacheConfig::new(4, 1, 16, Replacement::Lru, WritePolicy::WriteThrough);
        assert!(!config.write_allocate);
        let mut cache = Cache::new(config).unwrap();
        let miss = cache.access(0x40, CacheOp::Write);
        assert_eq!((miss.hit, miss.way), (false, None));
        assert!(cache.set(0).iter().all(|line| !line.valid));
        cache.access(0x40, CacheOp::Read);
        let hit = cache.access(0x48, CacheOp::Write);
        assert!(hit.hit);
        assert!(!cache.set(0)[0].dirty);
        assert_eq!(cache.stats().memory_writes, 2);
        assert_eq!(cache.stats().writebacks, 0);
    }

    #[test]
    fn write_back_without_allocation_writes_misses_to_memory() {
        let config = CacheConfig {
            write_allocate: false,
            ..CacheConfig::default()
        };
        let mut cache = Cache::new(config).unwrap();
        assert_eq!(cache.access(0x40, CacheOp::Write).way, None);
        assert_eq!(cache.stats().memory_writes, 1);
    }

    #[test]
    fn history_stays_bounded_and_evenly_spaced() {
        let mut cache = cache(4, 1, 4, Replacement::Lru);
        let addresses: Vec<u32> = (0..10_000).map(|i| (i % 8) * 4).collect();
        reads(&mut cache, &addresses);
        let history = cache.history();
        assert!(history.points.len() <= MAX_HISTORY_POINTS);
        assert!(history.points.len() > MAX_HISTORY_POINTS / 2);
        for (index, point) in history.points.iter().enumerate() {
            assert_eq!(point.accesses, history.stride * (index as u64 + 1));
            assert!(point.hits <= point.accesses);
        }
    }

    #[test]
    fn replacement_and_policy_ids_round_trip() {
        for replacement in Replacement::ALL {
            assert_eq!(Replacement::from_id(replacement.id()), Some(replacement));
        }
        for policy in WritePolicy::ALL {
            assert_eq!(WritePolicy::from_id(policy.id()), Some(policy));
        }
        assert_eq!(Replacement::from_id("plru"), None);
    }
}
