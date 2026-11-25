//! Mapa de memoria: qué tramos del espacio de 32 bits existen y qué se puede hacer en cada uno.

use rv_asm::Program;
use serde::Serialize;
use thiserror::Error;

/// Tamaño del segmento de datos estáticos más el montón, a partir de su base.
pub const DATA_SIZE: u32 = 0x1000_0000;
/// La pila ocupa `[STACK_BASE, STACK_END)` y crece hacia abajo desde arriba.
pub const STACK_BASE: u32 = 0x7000_0000;
pub const STACK_END: u64 = 0x8000_0000;
/// Valor inicial de `sp`: alineado a 16 como pide el convenio de llamada.
pub const INITIAL_SP: u32 = 0x7FFF_FFF0;
/// Zona de E/S mapeada en memoria: la parte alta del espacio de direcciones.
pub const IO_BASE: u32 = 0xFFFF_0000;
pub const IO_END: u64 = 0x1_0000_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentKind {
    Text,
    Data,
    Stack,
    Io,
}

impl SegmentKind {
    pub fn name(self) -> &'static str {
        match self {
            SegmentKind::Text => "texto",
            SegmentKind::Data => "datos",
            SegmentKind::Stack => "pila",
            SegmentKind::Io => "E/S",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl Permissions {
    pub const READ_EXECUTE: Permissions = Permissions {
        read: true,
        write: false,
        execute: true,
    };
    pub const READ_WRITE: Permissions = Permissions {
        read: true,
        write: true,
        execute: false,
    };
}

/// Un tramo `[base, end)` del espacio de direcciones. `end` es `u64` para poder llegar a 2^32.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Region {
    pub kind: SegmentKind,
    pub base: u32,
    pub end: u64,
    pub permissions: Permissions,
}

impl Region {
    pub fn new(kind: SegmentKind, base: u32, end: u64, permissions: Permissions) -> Region {
        Region {
            kind,
            base,
            end,
            permissions,
        }
    }

    pub fn len(&self) -> u64 {
        self.end.saturating_sub(u64::from(self.base))
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Si el acceso `[address, address + size)` cabe entero en el tramo.
    pub fn contains(&self, address: u32, size: u32) -> bool {
        let start = u64::from(address);
        start >= u64::from(self.base) && start + u64::from(size) <= self.end
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize)]
pub enum MapError {
    #[error("el segmento de {name} está vacío o al revés (0x{base:08x}..0x{end:09x})", name = .kind.name())]
    InvalidRegion {
        kind: SegmentKind,
        base: u32,
        end: u64,
    },
    #[error("los segmentos de {first} y {second} se solapan", first = .first.name(), second = .second.name())]
    Overlap {
        first: SegmentKind,
        second: SegmentKind,
    },
    #[error("el programa no cabe: su segmento de {name} se sale del mapa de memoria", name = .kind.name())]
    ProgramOutside { kind: SegmentKind },
}

/// Conjunto de tramos sin solapes, ordenados por dirección.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MemoryMap {
    regions: Vec<Region>,
    initial_sp: u32,
}

impl MemoryMap {
    /// Comprueba que los tramos no estén vacíos ni se solapen y los ordena.
    pub fn new(mut regions: Vec<Region>, initial_sp: u32) -> Result<MemoryMap, MapError> {
        for region in &regions {
            if region.end <= u64::from(region.base) || region.end > IO_END {
                return Err(MapError::InvalidRegion {
                    kind: region.kind,
                    base: region.base,
                    end: region.end,
                });
            }
        }
        regions.sort_by_key(|region| region.base);
        for pair in regions.windows(2) {
            if let [first, second] = pair
                && first.end > u64::from(second.base)
            {
                return Err(MapError::Overlap {
                    first: first.kind,
                    second: second.kind,
                });
            }
        }
        Ok(MemoryMap {
            regions,
            initial_sp,
        })
    }

    /// Mapa por defecto para un programa ensamblado: el texto ocupa justo lo que ocupa el código
    /// (salirse de él es un error, no ejecutar ceros), datos y montón desde la base de datos, la
    /// pila bajo 0x80000000 y la E/S en lo más alto.
    pub fn for_program(program: &Program) -> Result<MemoryMap, MapError> {
        let mut regions = Vec::with_capacity(4);
        if !program.text.bytes.is_empty() {
            regions.push(Region::new(
                SegmentKind::Text,
                program.text.base,
                program.text.end(),
                Permissions::READ_EXECUTE,
            ));
        }
        let data_base = program.layout.data_base;
        regions.push(Region::new(
            SegmentKind::Data,
            data_base,
            u64::from(data_base) + u64::from(DATA_SIZE),
            Permissions::READ_WRITE,
        ));
        regions.push(Region::new(
            SegmentKind::Stack,
            STACK_BASE,
            STACK_END,
            Permissions::READ_WRITE,
        ));
        regions.push(Region::new(
            SegmentKind::Io,
            IO_BASE,
            IO_END,
            Permissions::READ_WRITE,
        ));
        let map = MemoryMap::new(regions, INITIAL_SP)?;
        if !program.data.bytes.is_empty() && !map.covers(SegmentKind::Data, &program.data) {
            return Err(MapError::ProgramOutside {
                kind: SegmentKind::Data,
            });
        }
        Ok(map)
    }

    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    pub fn initial_sp(&self) -> u32 {
        self.initial_sp
    }

    /// Tramo que contiene `address`, si lo hay.
    pub fn region_at(&self, address: u32) -> Option<&Region> {
        let index = self
            .regions
            .partition_point(|region| region.base <= address);
        index
            .checked_sub(1)
            .and_then(|index| self.regions.get(index))
            .filter(|region| region.contains(address, 1))
    }

    pub fn region_of(&self, kind: SegmentKind) -> Option<&Region> {
        self.regions.iter().find(|region| region.kind == kind)
    }

    fn covers(&self, kind: SegmentKind, segment: &rv_asm::Segment) -> bool {
        self.region_of(kind).is_some_and(|region| {
            u64::from(segment.base) >= u64::from(region.base) && segment.end() <= region.end
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(kind: SegmentKind, base: u32, end: u64) -> Region {
        Region::new(kind, base, end, Permissions::READ_WRITE)
    }

    #[test]
    fn regions_are_sorted_and_looked_up_by_address() {
        let map = MemoryMap::new(
            vec![
                region(SegmentKind::Stack, 0x7000_0000, 0x8000_0000),
                region(SegmentKind::Data, 0x1000_0000, 0x1000_1000),
            ],
            INITIAL_SP,
        )
        .unwrap();
        assert_eq!(map.regions()[0].kind, SegmentKind::Data);
        assert_eq!(map.region_at(0x1000_0fff).unwrap().kind, SegmentKind::Data);
        assert!(map.region_at(0x1000_1000).is_none());
        assert!(map.region_at(0x0fff_ffff).is_none());
        assert_eq!(map.region_at(0x7fff_ffff).unwrap().kind, SegmentKind::Stack);
        assert!(map.region_at(0x8000_0000).is_none());
    }

    #[test]
    fn overlapping_or_empty_regions_are_rejected() {
        let overlap = MemoryMap::new(
            vec![
                region(SegmentKind::Data, 0x1000, 0x2001),
                region(SegmentKind::Stack, 0x2000, 0x3000),
            ],
            0,
        );
        assert_eq!(
            overlap,
            Err(MapError::Overlap {
                first: SegmentKind::Data,
                second: SegmentKind::Stack
            })
        );
        let empty = MemoryMap::new(vec![region(SegmentKind::Data, 0x1000, 0x1000)], 0);
        assert!(matches!(empty, Err(MapError::InvalidRegion { .. })));
        let too_far = MemoryMap::new(vec![region(SegmentKind::Io, 0xffff_0000, IO_END + 1)], 0);
        assert!(matches!(too_far, Err(MapError::InvalidRegion { .. })));
    }

    #[test]
    fn io_region_reaches_the_last_address() {
        let map = MemoryMap::new(vec![region(SegmentKind::Io, IO_BASE, IO_END)], 0).unwrap();
        let io = map.region_at(0xffff_ffff).unwrap();
        assert!(io.contains(0xffff_fffc, 4));
        assert!(!io.contains(0xffff_fffe, 4));
    }

    #[test]
    fn default_map_follows_the_program() {
        let program = rv_asm::assemble(".data\nx: .word 7\n.text\nnop\nnop\n").unwrap();
        let map = MemoryMap::for_program(&program).unwrap();
        let text = map.region_of(SegmentKind::Text).unwrap();
        assert_eq!((text.base, text.end), (0, 8));
        assert!(!text.permissions.write && text.permissions.execute);
        assert_eq!(map.region_at(0x1000_0000).unwrap().kind, SegmentKind::Data);
        assert_eq!(map.region_at(INITIAL_SP).unwrap().kind, SegmentKind::Stack);
        assert_eq!(map.region_at(0xffff_0000).unwrap().kind, SegmentKind::Io);
        assert!(map.region_at(8).is_none());
    }

    #[test]
    fn program_without_code_has_no_text_region() {
        let program = rv_asm::assemble(".data\n.word 1\n").unwrap();
        let map = MemoryMap::for_program(&program).unwrap();
        assert!(map.region_of(SegmentKind::Text).is_none());
    }
}
