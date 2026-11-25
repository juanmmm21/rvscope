//! Memoria little-endian dispersa con accesos de 1, 2 y 4 bytes comprobados contra el mapa.

use std::collections::BTreeMap;

use rv_asm::Program;
use serde::Serialize;
use thiserror::Error;

use crate::map::{MapError, MemoryMap, SegmentKind};

/// Las páginas solo existen cuando se escriben: un programa pequeño con la pila en 0x7fff_fff0
/// no reserva 2 GiB, y clonar la memoria para las instantáneas del paso atrás sale barato.
const PAGE_BITS: u32 = 12;
const PAGE_SIZE: usize = 1 << PAGE_BITS;
const PAGE_MASK: u32 = (1 << PAGE_BITS) - 1;

type Page = Box<[u8; PAGE_SIZE]>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Width {
    Byte,
    Half,
    Word,
}

impl Width {
    pub fn bytes(self) -> u32 {
        match self {
            Width::Byte => 1,
            Width::Half => 2,
            Width::Word => 4,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Width::Byte => "byte",
            Width::Half => "media palabra",
            Width::Word => "palabra",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessKind {
    Fetch,
    Load,
    Store,
}

impl AccessKind {
    fn name(self) -> &'static str {
        match self {
            AccessKind::Fetch => "leer la instrucción",
            AccessKind::Load => "leer",
            AccessKind::Store => "escribir",
        }
    }
}

/// Excepción de un acceso a memoria; el procesador la convierte en una parada con mensaje.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq, Serialize)]
pub enum MemoryError {
    #[error(
        "no se puede {access} en 0x{address:08x}: no pertenece a ningún segmento",
        access = .access.name()
    )]
    Unmapped { address: u32, access: AccessKind },
    #[error(
        "no se puede {access} en 0x{address:08x}: una {width} debe estar alineada a {align} bytes",
        access = .access.name(), width = .width.name(), align = .width.bytes()
    )]
    Misaligned {
        address: u32,
        width: Width,
        access: AccessKind,
    },
    #[error(
        "no se puede {access} en 0x{address:08x}: el segmento de {segment} no lo permite",
        access = .access.name(), segment = .segment.name()
    )]
    PermissionDenied {
        address: u32,
        access: AccessKind,
        segment: SegmentKind,
    },
}

impl MemoryError {
    pub fn address(&self) -> u32 {
        match *self {
            MemoryError::Unmapped { address, .. }
            | MemoryError::Misaligned { address, .. }
            | MemoryError::PermissionDenied { address, .. } => address,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Memory {
    map: MemoryMap,
    pages: BTreeMap<u32, Page>,
}

impl Memory {
    /// Memoria vacía (todo a cero) con el mapa dado.
    pub fn new(map: MemoryMap) -> Memory {
        Memory {
            map,
            pages: BTreeMap::new(),
        }
    }

    /// Memoria con el mapa por defecto y los segmentos del programa ya copiados.
    pub fn with_program(program: &Program) -> Result<Memory, MapError> {
        let mut memory = Memory::new(MemoryMap::for_program(program)?);
        memory.poke(program.text.base, &program.text.bytes);
        memory.poke(program.data.base, &program.data.bytes);
        Ok(memory)
    }

    pub fn map(&self) -> &MemoryMap {
        &self.map
    }

    /// Lee la instrucción de `address`: alineada a 4 y en un segmento ejecutable.
    pub fn fetch(&self, address: u32) -> Result<u32, MemoryError> {
        self.check(address, Width::Word, AccessKind::Fetch)?;
        Ok(self.read_raw(address, Width::Word))
    }

    /// Lectura de datos extendida con ceros; extender el signo (`lb`, `lh`) es cosa del procesador.
    pub fn load(&self, address: u32, width: Width) -> Result<u32, MemoryError> {
        self.check(address, width, AccessKind::Load)?;
        Ok(self.read_raw(address, width))
    }

    /// Escribe los `width` bytes bajos de `value`.
    pub fn store(&mut self, address: u32, width: Width, value: u32) -> Result<(), MemoryError> {
        self.check(address, width, AccessKind::Store)?;
        let bytes = value.to_le_bytes();
        let size = width.bytes() as usize;
        for (offset, byte) in bytes.iter().take(size).enumerate() {
            self.write_byte(address.wrapping_add(offset as u32), *byte);
        }
        Ok(())
    }

    /// Bytes de `[address, address + len)` sin comprobar nada, para que la interfaz pinte la
    /// memoria; lo que no está en ningún segmento se lee como cero. Se para al llegar a 2^32.
    pub fn peek(&self, address: u32, len: u32) -> Vec<u8> {
        let end = (u64::from(address) + u64::from(len)).min(1 << 32);
        (u64::from(address)..end)
            .map(|current| self.read_byte(current as u32))
            .collect()
    }

    /// Escribe bytes sin comprobar permisos: carga del programa y periféricos movidos desde la
    /// interfaz. Lo que caería fuera de 2^32 se descarta.
    pub fn poke(&mut self, address: u32, bytes: &[u8]) {
        for (offset, byte) in bytes.iter().enumerate() {
            let Some(current) = u32::try_from(offset)
                .ok()
                .and_then(|offset| address.checked_add(offset))
            else {
                break;
            };
            self.write_byte(current, *byte);
        }
    }

    pub(crate) fn check(
        &self,
        address: u32,
        width: Width,
        access: AccessKind,
    ) -> Result<(), MemoryError> {
        let size = width.bytes();
        if address % size != 0 {
            return Err(MemoryError::Misaligned {
                address,
                width,
                access,
            });
        }
        let region = self
            .map
            .region_at(address)
            .filter(|region| region.contains(address, size))
            .ok_or(MemoryError::Unmapped { address, access })?;
        let allowed = match access {
            AccessKind::Fetch => region.permissions.execute,
            AccessKind::Load => region.permissions.read,
            AccessKind::Store => region.permissions.write,
        };
        if allowed {
            Ok(())
        } else {
            Err(MemoryError::PermissionDenied {
                address,
                access,
                segment: region.kind,
            })
        }
    }

    fn read_raw(&self, address: u32, width: Width) -> u32 {
        let mut bytes = [0u8; 4];
        let size = width.bytes() as usize;
        for (offset, byte) in bytes.iter_mut().take(size).enumerate() {
            *byte = self.read_byte(address.wrapping_add(offset as u32));
        }
        u32::from_le_bytes(bytes)
    }

    fn read_byte(&self, address: u32) -> u8 {
        self.pages
            .get(&(address >> PAGE_BITS))
            .and_then(|page| page.get((address & PAGE_MASK) as usize))
            .copied()
            .unwrap_or(0)
    }

    fn write_byte(&mut self, address: u32, value: u8) {
        let page = self
            .pages
            .entry(address >> PAGE_BITS)
            .or_insert_with(|| Box::new([0; PAGE_SIZE]));
        if let Some(slot) = page.get_mut((address & PAGE_MASK) as usize) {
            *slot = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{INITIAL_SP, IO_BASE};

    fn memory(source: &str) -> Memory {
        Memory::with_program(&rv_asm::assemble(source).unwrap()).unwrap()
    }

    #[test]
    fn program_is_loaded_and_fetched_little_endian() {
        let program = rv_asm::assemble("addi a0, zero, 5\nret\n").unwrap();
        let memory = Memory::with_program(&program).unwrap();
        assert_eq!(memory.fetch(0).unwrap(), program.listing[0].word);
        assert_eq!(memory.fetch(4).unwrap(), program.listing[1].word);
        assert_eq!(memory.peek(0, 4), program.text.bytes[..4].to_vec());
    }

    #[test]
    fn loads_are_zero_extended_and_little_endian() {
        let memory = memory(".data\n.word 0x80ff1234\n.text\nnop\n");
        assert_eq!(memory.load(0x1000_0000, Width::Word).unwrap(), 0x80ff_1234);
        assert_eq!(memory.load(0x1000_0000, Width::Half).unwrap(), 0x1234);
        assert_eq!(memory.load(0x1000_0002, Width::Half).unwrap(), 0x80ff);
        assert_eq!(memory.load(0x1000_0003, Width::Byte).unwrap(), 0x80);
    }

    #[test]
    fn stores_write_only_the_low_bytes() {
        let mut memory = memory("nop\n");
        memory.store(0x1000_0000, Width::Word, 0xaabb_ccdd).unwrap();
        memory.store(0x1000_0001, Width::Byte, 0x1234_5611).unwrap();
        memory.store(0x1000_0002, Width::Half, 0xffff_2233).unwrap();
        assert_eq!(memory.peek(0x1000_0000, 4), vec![0xdd, 0x11, 0x33, 0x22]);
        assert_eq!(memory.load(0x1000_0000, Width::Word).unwrap(), 0x2233_11dd);
    }

    #[test]
    fn stack_and_io_are_writable() {
        let mut memory = memory("nop\n");
        memory.store(INITIAL_SP - 4, Width::Word, 42).unwrap();
        assert_eq!(memory.load(INITIAL_SP - 4, Width::Word).unwrap(), 42);
        memory.store(0xffff_fffc, Width::Word, 7).unwrap();
        assert_eq!(memory.load(0xffff_fffc, Width::Word).unwrap(), 7);
        assert_eq!(memory.load(IO_BASE, Width::Byte).unwrap(), 0);
    }

    #[test]
    fn misaligned_accesses_raise_an_exception() {
        let mut memory = memory("nop\nnop\n");
        assert_eq!(
            memory.load(0x1000_0001, Width::Half),
            Err(MemoryError::Misaligned {
                address: 0x1000_0001,
                width: Width::Half,
                access: AccessKind::Load
            })
        );
        assert!(matches!(
            memory.store(0x1000_0002, Width::Word, 0),
            Err(MemoryError::Misaligned { .. })
        ));
        assert!(matches!(
            memory.fetch(2),
            Err(MemoryError::Misaligned { .. })
        ));
        assert!(memory.load(0x1000_0001, Width::Byte).is_ok());
    }

    #[test]
    fn unmapped_addresses_raise_an_exception() {
        let mut memory = memory("nop\n");
        assert_eq!(
            memory.fetch(4),
            Err(MemoryError::Unmapped {
                address: 4,
                access: AccessKind::Fetch
            })
        );
        assert!(matches!(
            memory.load(0x2000_0000, Width::Word),
            Err(MemoryError::Unmapped { .. })
        ));
        assert!(matches!(
            memory.store(0x8000_0000, Width::Byte, 1),
            Err(MemoryError::Unmapped { .. })
        ));
    }

    #[test]
    fn permissions_are_enforced_per_segment() {
        let mut memory = memory("nop\n");
        assert_eq!(
            memory.store(0, Width::Word, 0),
            Err(MemoryError::PermissionDenied {
                address: 0,
                access: AccessKind::Store,
                segment: SegmentKind::Text
            })
        );
        assert!(memory.load(0, Width::Word).is_ok());
        assert!(matches!(
            memory.fetch(0x1000_0000),
            Err(MemoryError::PermissionDenied {
                segment: SegmentKind::Data,
                ..
            })
        ));
    }

    #[test]
    fn errors_read_well_in_spanish() {
        let memory = memory("nop\n");
        let error = memory.load(0x1000_0002, Width::Word).unwrap_err();
        assert_eq!(
            error.to_string(),
            "no se puede leer en 0x10000002: una palabra debe estar alineada a 4 bytes"
        );
        assert_eq!(error.address(), 0x1000_0002);
        let error = memory.fetch(0x1000_0000).unwrap_err();
        assert_eq!(
            error.to_string(),
            "no se puede leer la instrucción en 0x10000000: el segmento de datos no lo permite"
        );
    }

    #[test]
    fn accesses_across_pages_and_peek_at_the_top() {
        let mut memory = memory("nop\n");
        memory.poke(0x1000_0ffe, &[1, 2, 3, 4]);
        assert_eq!(memory.load(0x1000_0ffc, Width::Word).unwrap(), 0x0201_0000);
        assert_eq!(memory.load(0x1000_1000, Width::Half).unwrap(), 0x0403);
        memory.poke(0xffff_fffe, &[9, 9, 9, 9]);
        assert_eq!(memory.peek(0xffff_fffc, 16), vec![0, 0, 9, 9]);
    }

    #[test]
    fn clones_are_independent_snapshots() {
        let mut memory = memory("nop\n");
        memory.store(0x1000_0000, Width::Word, 1).unwrap();
        let snapshot = memory.clone();
        memory.store(0x1000_0000, Width::Word, 2).unwrap();
        assert_eq!(snapshot.load(0x1000_0000, Width::Word).unwrap(), 1);
        assert_ne!(snapshot, memory);
    }
}
