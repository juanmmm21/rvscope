//! Ficha de cada instrucción: sus campos de bits con los valores fijos que la identifican y un
//! ejemplo de uso. Sale de la misma tabla que el codificador, así que la chuleta enseña
//! exactamente los bits que rvscope genera.

use serde::Serialize;

use crate::format::{BitField, Format};
use crate::table::{InstructionSpec, Mnemonic, Operands};

/// Campo de bits de una instrucción concreta, de `high` a `low` ambos incluidos.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct EncodedField {
    pub name: &'static str,
    pub high: u8,
    pub low: u8,
    /// Valor fijo que identifica la instrucción (opcode, funct3…); `None` si lo pone el
    /// programa (registros e inmediatos).
    pub value: Option<u32>,
}

impl EncodedField {
    pub fn width(&self) -> u8 {
        self.high - self.low + 1
    }
}

const fn free(name: &'static str, high: u8, low: u8) -> EncodedField {
    EncodedField {
        name,
        high,
        low,
        value: None,
    }
}

impl InstructionSpec {
    /// Campos de la instrucción, del bit más alto al más bajo y sin huecos.
    ///
    /// Parte de los campos del formato, pero el formato I no cuenta toda la verdad: en los
    /// desplazamientos el inmediato se parte en `funct7` y `shamt`, en `ecall`/`ebreak` es un
    /// `funct12` fijo con `rs1` y `rd` a cero, y en `fence` lleva los conjuntos `pred` y `succ`.
    pub fn fields(&self) -> Vec<EncodedField> {
        let encoding = &self.encoding;
        let system = encoding.funct12.is_some();
        let mut fields = Vec::with_capacity(7);
        for field in self.format.fields() {
            match (self.format, field.name) {
                (Format::I, "imm[11:0]") if self.operands == Operands::RdRs1Shamt => {
                    fields.push(EncodedField {
                        value: encoding.funct7.map(u32::from),
                        ..free("funct7", 31, 25)
                    });
                    fields.push(free("shamt", 24, 20));
                }
                (Format::I, "imm[11:0]") if self.operands == Operands::Fence => {
                    fields.push(free("fm", 31, 28));
                    fields.push(free("pred", 27, 24));
                    fields.push(free("succ", 23, 20));
                }
                (Format::I, "imm[11:0]") if system => fields.push(EncodedField {
                    value: encoding.funct12.map(u32::from),
                    ..free("funct12", 31, 20)
                }),
                _ => fields.push(self.encoded(field, system)),
            }
        }
        fields
    }

    fn encoded(&self, field: &BitField, system: bool) -> EncodedField {
        let encoding = &self.encoding;
        let value = match field.name {
            "opcode" => Some(u32::from(encoding.opcode)),
            "funct3" => encoding.funct3.map(u32::from),
            "funct7" => encoding.funct7.map(u32::from),
            "rs1" | "rd" if system => Some(0),
            _ => None,
        };
        EncodedField {
            name: field.name,
            high: field.high,
            low: field.low,
            value,
        }
    }

    /// Uso concreto que el ensamblador acepta (los saltos van a una etiqueta `label`).
    pub fn example(&self) -> &'static str {
        example(self.mnemonic)
    }
}

// Un `match` exhaustivo en vez de un campo más en la tabla: así añadir una instrucción sin
// ejemplo no compila, y la tabla no gana otra columna que casi nadie lee al decodificar.
fn example(mnemonic: Mnemonic) -> &'static str {
    use Mnemonic as M;
    match mnemonic {
        M::Lui => "lui a0, 0x12345",
        M::Auipc => "auipc t0, 0x1",
        M::Jal => "jal ra, label",
        M::Jalr => "jalr ra, 0(t0)",
        M::Beq => "beq a0, a1, label",
        M::Bne => "bne t0, zero, label",
        M::Blt => "blt a0, a1, label",
        M::Bge => "bge t0, t1, label",
        M::Bltu => "bltu a0, a1, label",
        M::Bgeu => "bgeu t0, a1, label",
        M::Lb => "lb t0, 0(a0)",
        M::Lh => "lh t0, 2(a0)",
        M::Lw => "lw a0, 8(sp)",
        M::Lbu => "lbu t0, 1(a0)",
        M::Lhu => "lhu t0, 4(a0)",
        M::Sb => "sb t0, 0(a0)",
        M::Sh => "sh t0, 2(a0)",
        M::Sw => "sw ra, 12(sp)",
        M::Addi => "addi sp, sp, -16",
        M::Slti => "slti t0, a0, 10",
        M::Sltiu => "sltiu t0, a0, 1",
        M::Xori => "xori a0, a0, -1",
        M::Ori => "ori a0, a0, 0xff",
        M::Andi => "andi a0, a0, 0xf",
        M::Slli => "slli t0, a0, 2",
        M::Srli => "srli a0, a0, 4",
        M::Srai => "srai a0, a0, 1",
        M::Add => "add a0, a0, a1",
        M::Sub => "sub t0, a1, a0",
        M::Sll => "sll a0, a0, t0",
        M::Slt => "slt t0, a0, a1",
        M::Sltu => "sltu t0, a0, a1",
        M::Xor => "xor a0, a0, a1",
        M::Srl => "srl a0, a0, t0",
        M::Sra => "sra a0, a0, t0",
        M::Or => "or a0, a0, a1",
        M::And => "and a0, a0, a1",
        M::Fence => "fence rw, rw",
        M::Ecall => "ecall",
        M::Ebreak => "ebreak",
        M::Mul => "mul a0, a0, a1",
        M::Mulh => "mulh t0, a0, a1",
        M::Mulhsu => "mulhsu t0, a0, a1",
        M::Mulhu => "mulhu t0, a0, a1",
        M::Div => "div a0, a0, a1",
        M::Divu => "divu a0, a0, a1",
        M::Rem => "rem t0, a0, a1",
        M::Remu => "remu t0, a0, a1",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{INSTRUCTIONS, Instruction};

    #[test]
    fn fields_cover_32_bits_without_gaps() {
        for spec in &INSTRUCTIONS {
            let mut expected_high = 31u8;
            for field in spec.fields() {
                assert_eq!(field.high, expected_high, "{} {}", spec.name, field.name);
                assert!(field.low <= field.high);
                if let Some(value) = field.value {
                    assert!(value < 1 << field.width(), "{} {}", spec.name, field.name);
                }
                expected_high = field.low.wrapping_sub(1);
            }
            assert_eq!(expected_high, u8::MAX, "{}", spec.name);
        }
    }

    // Con solo los bits fijos (y ceros en el resto) sale la propia instrucción: la ficha
    // enseña todo lo que hace falta para reconocerla y nada que sobre.
    #[test]
    fn fixed_bits_identify_the_instruction() {
        for spec in &INSTRUCTIONS {
            let word = spec
                .fields()
                .iter()
                .filter_map(|field| field.value.map(|value| value << field.low))
                .fold(0u32, |word, bits| word | bits);
            let decoded = Instruction::decode(word);
            assert!(
                matches!(decoded, Ok(ref instruction) if instruction.mnemonic == spec.mnemonic),
                "{}: {word:#010x} -> {decoded:?}",
                spec.name
            );
        }
    }

    #[test]
    fn shifts_and_system_split_the_immediate() {
        let names = |mnemonic: Mnemonic| -> Vec<&str> {
            mnemonic
                .spec()
                .fields()
                .iter()
                .map(|field| field.name)
                .collect()
        };
        assert_eq!(
            names(Mnemonic::Srai),
            ["funct7", "shamt", "rs1", "funct3", "rd", "opcode"]
        );
        assert_eq!(
            names(Mnemonic::Ecall),
            ["funct12", "rs1", "funct3", "rd", "opcode"]
        );
        assert_eq!(
            names(Mnemonic::Fence),
            ["fm", "pred", "succ", "rs1", "funct3", "rd", "opcode"]
        );
        let srai = Mnemonic::Srai.spec().fields();
        assert_eq!(srai[0].value, Some(0b010_0000));
        let ebreak = Mnemonic::Ebreak.spec().fields();
        assert_eq!(ebreak[0].value, Some(1));
        assert_eq!(ebreak[1].value, Some(0));
    }

    #[test]
    fn every_example_starts_with_its_name() {
        for spec in &INSTRUCTIONS {
            let example = spec.example();
            let first = example.split_whitespace().next();
            assert_eq!(first, Some(spec.name), "{example}");
        }
    }
}
