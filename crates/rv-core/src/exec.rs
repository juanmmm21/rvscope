//! Semántica de RV32IM sin estado: qué hace cada instrucción con sus operandos. La comparten
//! todos los modelos de procesador, así que el resultado de una operación no puede divergir
//! entre ellos; lo único que cambia de un modelo a otro es cuándo se calcula.

use rv_isa::Mnemonic;
use serde::Serialize;

use crate::memory::Width;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AluOp {
    Add,
    Sub,
    Sll,
    Slt,
    Sltu,
    Xor,
    Srl,
    Sra,
    Or,
    And,
    Mul,
    Mulh,
    Mulhsu,
    Mulhu,
    Div,
    Divu,
    Rem,
    Remu,
}

impl AluOp {
    /// Resultado exacto según la especificación: aritmética modular, desplazamientos con los
    /// 5 bits bajos de `b` y los casos de división por cero y desbordamiento sin excepción.
    pub fn apply(self, a: u32, b: u32) -> u32 {
        let (sa, sb) = (a as i32, b as i32);
        let shamt = b & 0x1f;
        match self {
            AluOp::Add => a.wrapping_add(b),
            AluOp::Sub => a.wrapping_sub(b),
            AluOp::Sll => a << shamt,
            AluOp::Slt => u32::from(sa < sb),
            AluOp::Sltu => u32::from(a < b),
            AluOp::Xor => a ^ b,
            AluOp::Srl => a >> shamt,
            AluOp::Sra => (sa >> shamt) as u32,
            AluOp::Or => a | b,
            AluOp::And => a & b,
            AluOp::Mul => a.wrapping_mul(b),
            AluOp::Mulh => ((i64::from(sa) * i64::from(sb)) >> 32) as u32,
            // -2^31 · (2^32 - 1) cabe en i64, así que el producto mixto no desborda.
            AluOp::Mulhsu => ((i64::from(sa) * i64::from(b)) >> 32) as u32,
            AluOp::Mulhu => ((u64::from(a) * u64::from(b)) >> 32) as u32,
            AluOp::Div => {
                if b == 0 {
                    u32::MAX
                } else {
                    // i32::MIN / -1 desborda: la especificación pide devolver el dividendo.
                    sa.wrapping_div(sb) as u32
                }
            }
            AluOp::Divu => a.checked_div(b).unwrap_or(u32::MAX),
            AluOp::Rem => {
                if b == 0 {
                    a
                } else {
                    sa.wrapping_rem(sb) as u32
                }
            }
            AluOp::Remu => a.checked_rem(b).unwrap_or(a),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BranchCond {
    Eq,
    Ne,
    Lt,
    Ge,
    Ltu,
    Geu,
}

impl BranchCond {
    pub fn taken(self, a: u32, b: u32) -> bool {
        let (sa, sb) = (a as i32, b as i32);
        match self {
            BranchCond::Eq => a == b,
            BranchCond::Ne => a != b,
            BranchCond::Lt => sa < sb,
            BranchCond::Ge => sa >= sb,
            BranchCond::Ltu => a < b,
            BranchCond::Geu => a >= b,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct LoadOp {
    pub width: Width,
    pub signed: bool,
}

impl LoadOp {
    /// Extiende a 32 bits el valor que devuelve la memoria (que llega extendido con ceros).
    pub fn extend(self, raw: u32) -> u32 {
        match (self.width, self.signed) {
            (Width::Byte, true) => raw as u8 as i8 as i32 as u32,
            (Width::Half, true) => raw as u16 as i16 as i32 as u32,
            (Width::Byte, false) => raw & 0xff,
            (Width::Half, false) => raw & 0xffff,
            (Width::Word, _) => raw,
        }
    }
}

/// Segundo operando de la ALU: registro `rs2` o inmediato de la instrucción.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AluSource {
    Register,
    Immediate,
}

/// Qué hace una instrucción, ya clasificada: es lo que decide la unidad de control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Alu(AluOp, AluSource),
    Lui,
    Auipc,
    Jal,
    Jalr,
    Branch(BranchCond),
    Load(LoadOp),
    Store(Width),
    Fence,
    Ecall,
    Ebreak,
}

impl Operation {
    pub fn of(mnemonic: Mnemonic) -> Operation {
        use AluOp as A;
        use AluSource::{Immediate as Imm, Register as Reg};
        use Mnemonic as M;
        use Operation as O;
        let load = |width, signed| O::Load(LoadOp { width, signed });
        match mnemonic {
            M::Lui => O::Lui,
            M::Auipc => O::Auipc,
            M::Jal => O::Jal,
            M::Jalr => O::Jalr,
            M::Beq => O::Branch(BranchCond::Eq),
            M::Bne => O::Branch(BranchCond::Ne),
            M::Blt => O::Branch(BranchCond::Lt),
            M::Bge => O::Branch(BranchCond::Ge),
            M::Bltu => O::Branch(BranchCond::Ltu),
            M::Bgeu => O::Branch(BranchCond::Geu),
            M::Lb => load(Width::Byte, true),
            M::Lh => load(Width::Half, true),
            M::Lw => load(Width::Word, true),
            M::Lbu => load(Width::Byte, false),
            M::Lhu => load(Width::Half, false),
            M::Sb => O::Store(Width::Byte),
            M::Sh => O::Store(Width::Half),
            M::Sw => O::Store(Width::Word),
            M::Addi => O::Alu(A::Add, Imm),
            M::Slti => O::Alu(A::Slt, Imm),
            M::Sltiu => O::Alu(A::Sltu, Imm),
            M::Xori => O::Alu(A::Xor, Imm),
            M::Ori => O::Alu(A::Or, Imm),
            M::Andi => O::Alu(A::And, Imm),
            M::Slli => O::Alu(A::Sll, Imm),
            M::Srli => O::Alu(A::Srl, Imm),
            M::Srai => O::Alu(A::Sra, Imm),
            M::Add => O::Alu(A::Add, Reg),
            M::Sub => O::Alu(A::Sub, Reg),
            M::Sll => O::Alu(A::Sll, Reg),
            M::Slt => O::Alu(A::Slt, Reg),
            M::Sltu => O::Alu(A::Sltu, Reg),
            M::Xor => O::Alu(A::Xor, Reg),
            M::Srl => O::Alu(A::Srl, Reg),
            M::Sra => O::Alu(A::Sra, Reg),
            M::Or => O::Alu(A::Or, Reg),
            M::And => O::Alu(A::And, Reg),
            M::Fence => O::Fence,
            M::Ecall => O::Ecall,
            M::Ebreak => O::Ebreak,
            M::Mul => O::Alu(A::Mul, Reg),
            M::Mulh => O::Alu(A::Mulh, Reg),
            M::Mulhsu => O::Alu(A::Mulhsu, Reg),
            M::Mulhu => O::Alu(A::Mulhu, Reg),
            M::Div => O::Alu(A::Div, Reg),
            M::Divu => O::Alu(A::Divu, Reg),
            M::Rem => O::Alu(A::Rem, Reg),
            M::Remu => O::Alu(A::Remu, Reg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rv_isa::INSTRUCTIONS;

    const MIN: u32 = i32::MIN as u32;
    const NEG1: u32 = u32::MAX;

    #[test]
    fn arithmetic_wraps_and_shifts_are_masked() {
        assert_eq!(AluOp::Add.apply(u32::MAX, 1), 0);
        assert_eq!(AluOp::Sub.apply(0, 1), u32::MAX);
        assert_eq!(AluOp::Sll.apply(1, 33), 2);
        assert_eq!(AluOp::Srl.apply(MIN, 31), 1);
        assert_eq!(AluOp::Sra.apply(MIN, 31), NEG1);
        assert_eq!(AluOp::Sra.apply(MIN, 0x20), MIN);
    }

    #[test]
    fn comparisons_distinguish_signed_and_unsigned() {
        assert_eq!(AluOp::Slt.apply(NEG1, 1), 1);
        assert_eq!(AluOp::Sltu.apply(NEG1, 1), 0);
        assert!(BranchCond::Lt.taken(NEG1, 0));
        assert!(!BranchCond::Ltu.taken(NEG1, 0));
        assert!(BranchCond::Geu.taken(NEG1, 0));
        assert!(BranchCond::Ge.taken(5, 5));
        assert!(BranchCond::Ne.taken(1, 2) && !BranchCond::Eq.taken(1, 2));
    }

    #[test]
    fn multiplication_high_halves() {
        assert_eq!(AluOp::Mul.apply(0x1_0001, 0x1_0001), 0x2_0001);
        assert_eq!(AluOp::Mulh.apply(NEG1, NEG1), 0);
        assert_eq!(AluOp::Mulhu.apply(NEG1, NEG1), 0xffff_fffe);
        assert_eq!(AluOp::Mulhsu.apply(NEG1, NEG1), NEG1);
        assert_eq!(AluOp::Mulh.apply(MIN, MIN), 0x4000_0000);
        assert_eq!(AluOp::Mulhsu.apply(MIN, NEG1), 0x8000_0000);
    }

    #[test]
    fn division_corner_cases_follow_the_specification() {
        assert_eq!(AluOp::Div.apply(7, 0), NEG1);
        assert_eq!(AluOp::Divu.apply(7, 0), u32::MAX);
        assert_eq!(AluOp::Rem.apply(7, 0), 7);
        assert_eq!(AluOp::Remu.apply(7, 0), 7);
        assert_eq!(AluOp::Div.apply(MIN, NEG1), MIN);
        assert_eq!(AluOp::Rem.apply(MIN, NEG1), 0);
        assert_eq!(AluOp::Div.apply(-7i32 as u32, 2), -3i32 as u32);
        assert_eq!(AluOp::Rem.apply(-7i32 as u32, 2), NEG1);
        assert_eq!(AluOp::Divu.apply(-7i32 as u32, 2), 0x7fff_fffc);
    }

    #[test]
    fn loads_extend_by_width_and_sign() {
        let lb = LoadOp {
            width: Width::Byte,
            signed: true,
        };
        let lhu = LoadOp {
            width: Width::Half,
            signed: false,
        };
        let lh = LoadOp {
            width: Width::Half,
            signed: true,
        };
        assert_eq!(lb.extend(0x80), 0xffff_ff80);
        assert_eq!(lb.extend(0x7f), 0x7f);
        assert_eq!(lh.extend(0x8000), 0xffff_8000);
        assert_eq!(lhu.extend(0x8000), 0x8000);
    }

    #[test]
    fn immediate_operations_use_the_immediate_source() {
        for spec in &INSTRUCTIONS {
            if let Operation::Alu(_, source) = Operation::of(spec.mnemonic) {
                let expected = if spec.format == rv_isa::Format::R {
                    AluSource::Register
                } else {
                    AluSource::Immediate
                };
                assert_eq!(source, expected, "{}", spec.name);
            }
        }
    }
}
