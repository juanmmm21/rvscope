//! Resultado del ensamblado: segmentos, símbolos y listado.

use std::collections::BTreeMap;

use rv_isa::Instruction;
use serde::Serialize;

/// Direcciones base de los segmentos.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Layout {
    pub text_base: u32,
    pub data_base: u32,
}

impl Default for Layout {
    fn default() -> Layout {
        Layout {
            text_base: 0x0000_0000,
            data_base: 0x1000_0000,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Segment {
    pub base: u32,
    pub bytes: Vec<u8>,
}

impl Segment {
    /// Primera dirección después del segmento.
    pub fn end(&self) -> u64 {
        u64::from(self.base) + self.bytes.len() as u64
    }
}

/// Una instrucción emitida, con la línea de la que sale.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ListingEntry {
    pub address: u32,
    pub word: u32,
    pub instruction: Instruction,
    /// Línea de la fuente, desde 1.
    pub line: usize,
    /// Pseudoinstrucción de la que sale, si no se escribió como instrucción real.
    pub pseudo: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Program {
    pub layout: Layout,
    pub text: Segment,
    pub data: Segment,
    pub entry: u32,
    /// Etiquetas y su dirección (las constantes de `.equ` no ocupan memoria y no aparecen).
    pub labels: BTreeMap<String, u32>,
    pub listing: Vec<ListingEntry>,
}

impl Program {
    /// Línea de la fuente de la instrucción en `address`.
    pub fn line_at(&self, address: u32) -> Option<usize> {
        self.entry_at(address).map(|entry| entry.line)
    }

    pub fn entry_at(&self, address: u32) -> Option<&ListingEntry> {
        self.listing
            .binary_search_by_key(&address, |entry| entry.address)
            .ok()
            .and_then(|index| self.listing.get(index))
    }

    /// Direcciones de las instrucciones que salen de una línea (varias si es una pseudoinstrucción).
    pub fn addresses_of_line(&self, line: usize) -> Vec<u32> {
        self.listing
            .iter()
            .filter(|entry| entry.line == line)
            .map(|entry| entry.address)
            .collect()
    }
}
