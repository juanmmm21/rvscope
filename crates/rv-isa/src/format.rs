//! Formatos de codificación de 32 bits y sus campos, para la chuleta y el desensamblado.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum Format {
    R,
    I,
    S,
    B,
    U,
    J,
}

/// Campo de bits de un formato, de `high` a `low` ambos incluidos.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct BitField {
    pub name: &'static str,
    pub high: u8,
    pub low: u8,
}

const fn field(name: &'static str, high: u8, low: u8) -> BitField {
    BitField { name, high, low }
}

const OPCODE: BitField = field("opcode", 6, 0);
const RD: BitField = field("rd", 11, 7);
const FUNCT3: BitField = field("funct3", 14, 12);
const RS1: BitField = field("rs1", 19, 15);
const RS2: BitField = field("rs2", 24, 20);

const R_FIELDS: [BitField; 6] = [field("funct7", 31, 25), RS2, RS1, FUNCT3, RD, OPCODE];
const I_FIELDS: [BitField; 5] = [field("imm[11:0]", 31, 20), RS1, FUNCT3, RD, OPCODE];
const S_FIELDS: [BitField; 6] = [
    field("imm[11:5]", 31, 25),
    RS2,
    RS1,
    FUNCT3,
    field("imm[4:0]", 11, 7),
    OPCODE,
];
const B_FIELDS: [BitField; 6] = [
    field("imm[12|10:5]", 31, 25),
    RS2,
    RS1,
    FUNCT3,
    field("imm[4:1|11]", 11, 7),
    OPCODE,
];
const U_FIELDS: [BitField; 3] = [field("imm[31:12]", 31, 12), RD, OPCODE];
const J_FIELDS: [BitField; 3] = [field("imm[20|10:1|11|19:12]", 31, 12), RD, OPCODE];

impl Format {
    /// Campos del formato, del bit más alto al más bajo, cubriendo los 32 bits sin huecos.
    pub fn fields(self) -> &'static [BitField] {
        match self {
            Format::R => &R_FIELDS,
            Format::I => &I_FIELDS,
            Format::S => &S_FIELDS,
            Format::B => &B_FIELDS,
            Format::U => &U_FIELDS,
            Format::J => &J_FIELDS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_format_covers_32_bits_without_gaps() {
        for format in [
            Format::R,
            Format::I,
            Format::S,
            Format::B,
            Format::U,
            Format::J,
        ] {
            let mut expected_high = 31u8;
            for field in format.fields() {
                assert_eq!(field.high, expected_high, "{format:?} {}", field.name);
                assert!(field.low <= field.high);
                expected_high = field.low.wrapping_sub(1);
            }
            assert_eq!(expected_high, u8::MAX, "{format:?} no llega al bit 0");
        }
    }
}
