//! Instrucción decodificada: paso de palabra de 32 bits a campos y vuelta.

use serde::Serialize;
use thiserror::Error;

use crate::format::Format;
use crate::register::{Reg, RegisterStyle};
use crate::table::{INSTRUCTIONS, InstructionSpec, Mnemonic, Operands};

/// Instrucción con sus operandos. Los registros que el formato no usa valen `x0`.
///
/// `imm` es siempre el valor que usa el hardware ya extendido: el desplazamiento en bytes en
/// saltos y accesos a memoria, `shamt` en los desplazamientos, el valor ya desplazado
/// (`imm << 12`) en `lui`/`auipc` y los bits `pred`/`succ` en `fence`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Instruction {
    pub mnemonic: Mnemonic,
    pub rd: Reg,
    pub rs1: Reg,
    pub rs2: Reg,
    pub imm: i32,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("instrucción comprimida 0x{0:04x}: la extensión C no está admitida")]
    Compressed(u16),
    #[error("instrucción ilegal o no admitida: 0x{0:08x}")]
    Illegal(u32),
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum EncodeError {
    #[error("{mnemonic}: el inmediato {value} está fuera de rango ({min} a {max})")]
    ImmediateOutOfRange {
        mnemonic: &'static str,
        value: i64,
        min: i64,
        max: i64,
    },
    #[error("{mnemonic}: el desplazamiento {value} debe ser par")]
    OddOffset { mnemonic: &'static str, value: i32 },
    #[error("{mnemonic}: el inmediato 0x{value:08x} tiene bits en los 12 de abajo")]
    UpperImmediateLowBits { mnemonic: &'static str, value: i32 },
}

/// Valor de los bits `high..=low` de `word`, ya desplazado a la posición 0.
const fn bits(word: u32, high: u32, low: u32) -> u32 {
    (word >> low) & ((1u32 << (high - low + 1)) - 1)
}

/// Extiende el signo de un valor de `width` bits.
const fn sign_extend(value: u32, width: u32) -> i32 {
    let shift = 32 - width;
    ((value << shift) as i32) >> shift
}

/// Inmediato tal como lo usa el hardware, según el formato y la forma de los operandos.
fn immediate_from_word(spec: &InstructionSpec, word: u32) -> i32 {
    match (spec.format, spec.operands) {
        (Format::R, _) | (_, Operands::None) => 0,
        (Format::I, Operands::RdRs1Shamt) => bits(word, 24, 20) as i32,
        (Format::I, Operands::Fence) => bits(word, 31, 20) as i32,
        (Format::I, _) => sign_extend(bits(word, 31, 20), 12),
        (Format::S, _) => sign_extend((bits(word, 31, 25) << 5) | bits(word, 11, 7), 12),
        (Format::B, _) => sign_extend(
            (bits(word, 31, 31) << 12)
                | (bits(word, 7, 7) << 11)
                | (bits(word, 30, 25) << 5)
                | (bits(word, 11, 8) << 1),
            13,
        ),
        (Format::U, _) => (word & 0xffff_f000) as i32,
        (Format::J, _) => sign_extend(
            (bits(word, 31, 31) << 20)
                | (bits(word, 19, 12) << 12)
                | (bits(word, 20, 20) << 11)
                | (bits(word, 30, 21) << 1),
            21,
        ),
    }
}

fn matches(spec: &InstructionSpec, word: u32) -> bool {
    let encoding = &spec.encoding;
    if bits(word, 6, 0) != u32::from(encoding.opcode) {
        return false;
    }
    if encoding
        .funct3
        .is_some_and(|f3| bits(word, 14, 12) != u32::from(f3))
    {
        return false;
    }
    if encoding
        .funct7
        .is_some_and(|f7| bits(word, 31, 25) != u32::from(f7))
    {
        return false;
    }
    if let Some(f12) = encoding.funct12 {
        // ecall y ebreak solo son válidas con rd y rs1 a cero; el resto del espacio está reservado.
        if bits(word, 31, 20) != u32::from(f12) || bits(word, 19, 15) != 0 || bits(word, 11, 7) != 0
        {
            return false;
        }
    }
    true
}

fn check_range(spec: &InstructionSpec, value: i32, min: i32, max: i32) -> Result<(), EncodeError> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(EncodeError::ImmediateOutOfRange {
            mnemonic: spec.name,
            value: i64::from(value),
            min: i64::from(min),
            max: i64::from(max),
        })
    }
}

fn check_even(spec: &InstructionSpec, value: i32) -> Result<(), EncodeError> {
    if value % 2 == 0 {
        Ok(())
    } else {
        Err(EncodeError::OddOffset {
            mnemonic: spec.name,
            value,
        })
    }
}

impl Instruction {
    pub fn new(mnemonic: Mnemonic, rd: Reg, rs1: Reg, rs2: Reg, imm: i32) -> Instruction {
        Instruction {
            mnemonic,
            rd,
            rs1,
            rs2,
            imm,
        }
    }

    pub fn spec(&self) -> &'static InstructionSpec {
        self.mnemonic.spec()
    }

    pub fn decode(word: u32) -> Result<Instruction, DecodeError> {
        if word & 0b11 != 0b11 {
            return Err(DecodeError::Compressed((word & 0xffff) as u16));
        }
        let spec = INSTRUCTIONS
            .iter()
            .find(|spec| matches(spec, word))
            .ok_or(DecodeError::Illegal(word))?;
        let imm = immediate_from_word(spec, word);
        let rd_field = Reg::from_field(bits(word, 11, 7));
        let rs1_field = Reg::from_field(bits(word, 19, 15));
        let rs2_field = Reg::from_field(bits(word, 24, 20));
        let (rd, rs1, rs2) = match spec.format {
            Format::R => (rd_field, rs1_field, rs2_field),
            Format::I => (rd_field, rs1_field, Reg::ZERO),
            Format::S | Format::B => (Reg::ZERO, rs1_field, rs2_field),
            Format::U | Format::J => (rd_field, Reg::ZERO, Reg::ZERO),
        };
        Ok(Instruction::new(spec.mnemonic, rd, rs1, rs2, imm))
    }

    pub fn encode(&self) -> Result<u32, EncodeError> {
        let spec = self.spec();
        let encoding = &spec.encoding;
        let opcode = u32::from(encoding.opcode);
        let funct3 = u32::from(encoding.funct3.unwrap_or(0)) << 12;
        let funct7 = u32::from(encoding.funct7.unwrap_or(0)) << 25;
        let rd = self.rd.bits() << 7;
        let rs1 = self.rs1.bits() << 15;
        let rs2 = self.rs2.bits() << 20;
        let imm = self.imm;
        let imm_bits = imm as u32;
        let word = match (spec.format, spec.operands) {
            (Format::R, _) => funct7 | rs2 | rs1 | funct3 | rd | opcode,
            (Format::I, Operands::RdRs1Shamt) => {
                check_range(spec, imm, 0, 31)?;
                funct7 | (imm_bits << 20) | rs1 | funct3 | rd | opcode
            }
            (Format::I, Operands::None) => {
                let funct12 = u32::from(encoding.funct12.unwrap_or(0));
                (funct12 << 20) | funct3 | opcode
            }
            (Format::I, Operands::Fence) => {
                check_range(spec, imm, 0, 0xff)?;
                (imm_bits << 20) | rs1 | funct3 | rd | opcode
            }
            (Format::I, _) => {
                check_range(spec, imm, -2048, 2047)?;
                ((imm_bits & 0xfff) << 20) | rs1 | funct3 | rd | opcode
            }
            (Format::S, _) => {
                check_range(spec, imm, -2048, 2047)?;
                (bits(imm_bits, 11, 5) << 25)
                    | rs2
                    | rs1
                    | funct3
                    | (bits(imm_bits, 4, 0) << 7)
                    | opcode
            }
            (Format::B, _) => {
                check_range(spec, imm, -4096, 4094)?;
                check_even(spec, imm)?;
                (bits(imm_bits, 12, 12) << 31)
                    | (bits(imm_bits, 10, 5) << 25)
                    | rs2
                    | rs1
                    | funct3
                    | (bits(imm_bits, 4, 1) << 8)
                    | (bits(imm_bits, 11, 11) << 7)
                    | opcode
            }
            (Format::U, _) => {
                if imm_bits & 0xfff != 0 {
                    return Err(EncodeError::UpperImmediateLowBits {
                        mnemonic: spec.name,
                        value: imm,
                    });
                }
                imm_bits | rd | opcode
            }
            (Format::J, _) => {
                check_range(spec, imm, -(1 << 20), (1 << 20) - 2)?;
                check_even(spec, imm)?;
                (bits(imm_bits, 20, 20) << 31)
                    | (bits(imm_bits, 10, 1) << 21)
                    | (bits(imm_bits, 11, 11) << 20)
                    | (bits(imm_bits, 19, 12) << 12)
                    | rd
                    | opcode
            }
        };
        Ok(word)
    }

    /// Texto en ensamblador. `pc` es la dirección de la instrucción: los saltos muestran la
    /// dirección de destino, que es lo que el ensamblador acepta de vuelta.
    pub fn disassemble(&self, pc: u32, style: RegisterStyle) -> String {
        let spec = self.spec();
        let name = spec.name;
        let rd = self.rd.name(style);
        let rs1 = self.rs1.name(style);
        let rs2 = self.rs2.name(style);
        let imm = self.imm;
        let target = pc.wrapping_add(imm as u32);
        match spec.operands {
            Operands::RdRs1Rs2 => format!("{name} {rd}, {rs1}, {rs2}"),
            Operands::RdRs1Imm | Operands::RdRs1Shamt => format!("{name} {rd}, {rs1}, {imm}"),
            Operands::RdOffsetRs1 => format!("{name} {rd}, {imm}({rs1})"),
            Operands::Rs2OffsetRs1 => format!("{name} {rs2}, {imm}({rs1})"),
            Operands::Rs1Rs2Target => format!("{name} {rs1}, {rs2}, 0x{target:x}"),
            Operands::RdUpper => format!("{name} {rd}, 0x{:x}", (imm as u32) >> 12),
            Operands::RdTarget => format!("{name} {rd}, 0x{target:x}"),
            Operands::Fence => {
                let pred = fence_set((imm >> 4) as u32);
                let succ = fence_set(imm as u32);
                if pred == "iorw" && succ == "iorw" {
                    name.to_owned()
                } else {
                    format!("{name} {pred}, {succ}")
                }
            }
            Operands::None => name.to_owned(),
        }
    }
}

/// Conjunto `iorw` de `fence` a partir de sus 4 bits (i = bit 3 ... w = bit 0).
pub fn fence_set(bits4: u32) -> String {
    let set: String = [(8, 'i'), (4, 'o'), (2, 'r'), (1, 'w')]
        .into_iter()
        .filter(|(mask, _)| bits4 & mask != 0)
        .map(|(_, letter)| letter)
        .collect();
    if set.is_empty() { "0".to_owned() } else { set }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::Mnemonic as M;

    fn reg(index: u8) -> Reg {
        Reg::new(index).unwrap_or_default()
    }

    /// Palabras de referencia comprobadas contra la codificación de la especificación.
    #[test]
    fn decodes_known_words() {
        let cases: [(u32, &str); 14] = [
            (0x0000_0013, "addi zero, zero, 0"),
            (0x00a0_0513, "addi a0, zero, 10"),
            (0xfff5_0513, "addi a0, a0, -1"),
            (0x00b5_0533, "add a0, a0, a1"),
            (0x40b5_0533, "sub a0, a0, a1"),
            (0x0081_2503, "lw a0, 8(sp)"),
            (0x00a1_2423, "sw a0, 8(sp)"),
            (0x1234_5537, "lui a0, 0x12345"),
            (0x4015_5513, "srai a0, a0, 1"),
            (0x02b5_0533, "mul a0, a0, a1"),
            (0x0000_0073, "ecall"),
            (0x0010_0073, "ebreak"),
            (0x0ff0_000f, "fence"),
            (0x0000_8067, "jalr zero, 0(ra)"),
        ];
        for (word, text) in cases {
            let decoded = Instruction::decode(word);
            assert_eq!(
                decoded.map(|i| i.disassemble(0, RegisterStyle::Abi)),
                Ok(text.to_owned()),
                "0x{word:08x}"
            );
        }
    }

    #[test]
    fn branch_and_jump_targets_are_absolute() {
        // beq a0, a1, +8 desde 0x100; jal ra, -4 desde 0x100.
        let beq = Instruction::decode(0x00b5_0463);
        assert_eq!(
            beq.map(|i| i.disassemble(0x100, RegisterStyle::Abi)),
            Ok("beq a0, a1, 0x108".to_owned())
        );
        let jal = Instruction::decode(0xffdf_f0ef);
        assert_eq!(
            jal.map(|i| i.disassemble(0x100, RegisterStyle::Numeric)),
            Ok("jal x1, 0xfc".to_owned())
        );
    }

    #[test]
    fn rejects_illegal_and_compressed_words() {
        assert_eq!(
            Instruction::decode(0x0000_0000),
            Err(DecodeError::Compressed(0))
        );
        assert_eq!(
            Instruction::decode(0xffff_ffff),
            Err(DecodeError::Illegal(0xffff_ffff))
        );
        // slli con el bit 25 a uno solo existe en RV64.
        assert_eq!(
            Instruction::decode(0x0215_1513),
            Err(DecodeError::Illegal(0x0215_1513))
        );
        // ecall con rd distinto de cero está reservado.
        assert_eq!(
            Instruction::decode(0x0000_00f3),
            Err(DecodeError::Illegal(0x0000_00f3))
        );
        // fence.i (Zifencei) no está admitida.
        assert_eq!(
            Instruction::decode(0x0000_100f),
            Err(DecodeError::Illegal(0x0000_100f))
        );
    }

    fn sample_immediates(spec: &InstructionSpec) -> Vec<i32> {
        match (spec.format, spec.operands) {
            (Format::R, _) | (Format::I, Operands::None) => vec![0],
            (Format::I, Operands::RdRs1Shamt) => vec![0, 1, 17, 31],
            (Format::I, Operands::Fence) => vec![0xff, 0x33, 0],
            (Format::I | Format::S, _) => vec![-2048, -1, 0, 1, 2047, 1234, -777],
            (Format::B, _) => vec![-4096, -2, 0, 2, 4094, 2048, -2050],
            (Format::U, _) => vec![0, 0x1234_5000, -4096, i32::MIN],
            (Format::J, _) => vec![-(1 << 20), -2, 0, 2, (1 << 20) - 2, 0x7_f802],
        }
    }

    #[test]
    fn encode_then_decode_round_trips_every_instruction() {
        for spec in &INSTRUCTIONS {
            for imm in sample_immediates(spec) {
                let (rd, rs1, rs2) = match (spec.format, spec.operands) {
                    (_, Operands::None) => (reg(0), reg(0), reg(0)),
                    (Format::R, _) => (reg(5), reg(17), reg(31)),
                    (Format::I, _) => (reg(31), reg(1), reg(0)),
                    (Format::S | Format::B, _) => (reg(0), reg(9), reg(30)),
                    (Format::U | Format::J, _) => (reg(12), reg(0), reg(0)),
                };
                let original = Instruction::new(spec.mnemonic, rd, rs1, rs2, imm);
                let word = original.encode();
                assert!(word.is_ok(), "{} {imm}: {word:?}", spec.name);
                let decoded = word
                    .map_err(|_| DecodeError::Illegal(0))
                    .and_then(Instruction::decode);
                assert_eq!(decoded, Ok(original), "{} {imm}", spec.name);
            }
        }
    }

    #[test]
    fn encode_rejects_out_of_range_immediates() {
        let addi = Instruction::new(M::Addi, reg(1), reg(1), reg(0), 2048);
        assert!(matches!(
            addi.encode(),
            Err(EncodeError::ImmediateOutOfRange {
                min: -2048,
                max: 2047,
                ..
            })
        ));
        let slli = Instruction::new(M::Slli, reg(1), reg(1), reg(0), 32);
        assert!(slli.encode().is_err());
        let beq = Instruction::new(M::Beq, reg(0), reg(1), reg(2), 3);
        assert!(matches!(beq.encode(), Err(EncodeError::OddOffset { .. })));
        let jal = Instruction::new(M::Jal, reg(1), reg(0), reg(0), 1 << 20);
        assert!(jal.encode().is_err());
        let lui = Instruction::new(M::Lui, reg(1), reg(0), reg(0), 0x123);
        assert!(matches!(
            lui.encode(),
            Err(EncodeError::UpperImmediateLowBits { .. })
        ));
    }

    #[test]
    fn fence_shows_its_sets_when_not_full() {
        let fence = Instruction::new(M::Fence, reg(0), reg(0), reg(0), 0x31);
        assert_eq!(fence.disassemble(0, RegisterStyle::Abi), "fence rw, w");
    }
}
