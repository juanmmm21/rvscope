//! Tabla única de instrucciones admitidas. De aquí salen el decodificador, el codificador,
//! el ensamblador y la chuleta de la interfaz, para que nunca se contradigan.

use serde::Serialize;

use crate::format::Format;

/// Instrucciones reales (no pseudoinstrucciones) de RV32I y la extensión M.
/// El orden coincide con el de [`INSTRUCTIONS`]; un test lo vigila.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mnemonic {
    Lui,
    Auipc,
    Jal,
    Jalr,
    Beq,
    Bne,
    Blt,
    Bge,
    Bltu,
    Bgeu,
    Lb,
    Lh,
    Lw,
    Lbu,
    Lhu,
    Sb,
    Sh,
    Sw,
    Addi,
    Slti,
    Sltiu,
    Xori,
    Ori,
    Andi,
    Slli,
    Srli,
    Srai,
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
    Fence,
    Ecall,
    Ebreak,
    Mul,
    Mulh,
    Mulhsu,
    Mulhu,
    Div,
    Divu,
    Rem,
    Remu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum Extension {
    /// Base entera de 32 bits.
    I,
    /// Multiplicación y división enteras.
    M,
}

/// Grupo de la chuleta. El orden de las variantes es el orden en que se muestran.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Arithmetic,
    Logical,
    Shift,
    Compare,
    Upper,
    Load,
    Store,
    Branch,
    Jump,
    Multiply,
    Divide,
    System,
}

impl Group {
    pub const ALL: [Group; 12] = [
        Group::Arithmetic,
        Group::Logical,
        Group::Shift,
        Group::Compare,
        Group::Upper,
        Group::Load,
        Group::Store,
        Group::Branch,
        Group::Jump,
        Group::Multiply,
        Group::Divide,
        Group::System,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Group::Arithmetic => "Aritméticas",
            Group::Logical => "Lógicas",
            Group::Shift => "Desplazamientos",
            Group::Compare => "Comparaciones",
            Group::Upper => "Inmediatos superiores",
            Group::Load => "Cargas",
            Group::Store => "Almacenamientos",
            Group::Branch => "Saltos condicionales",
            Group::Jump => "Saltos incondicionales",
            Group::Multiply => "Multiplicación (M)",
            Group::Divide => "División (M)",
            Group::System => "Sistema",
        }
    }
}

/// Forma de los operandos en ensamblador. Determina la sintaxis, cómo se ensambla y cómo
/// se desensambla.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operands {
    /// `rd, rs1, rs2`
    RdRs1Rs2,
    /// `rd, rs1, imm` con inmediato de 12 bits con signo.
    RdRs1Imm,
    /// `rd, rs1, shamt` con desplazamiento de 0 a 31.
    RdRs1Shamt,
    /// `rd, offset(rs1)`: cargas y `jalr`.
    RdOffsetRs1,
    /// `rs2, offset(rs1)`: almacenamientos.
    Rs2OffsetRs1,
    /// `rs1, rs2, etiqueta`: saltos condicionales.
    Rs1Rs2Target,
    /// `rd, imm` con los 20 bits altos.
    RdUpper,
    /// `rd, etiqueta`: `jal`.
    RdTarget,
    /// `[pred, succ]`: conjuntos de accesos ordenados por `fence`.
    Fence,
    /// Sin operandos.
    None,
}

impl Operands {
    pub fn syntax(self) -> &'static str {
        match self {
            Operands::RdRs1Rs2 => "rd, rs1, rs2",
            Operands::RdRs1Imm => "rd, rs1, imm",
            Operands::RdRs1Shamt => "rd, rs1, shamt",
            Operands::RdOffsetRs1 => "rd, offset(rs1)",
            Operands::Rs2OffsetRs1 => "rs2, offset(rs1)",
            Operands::Rs1Rs2Target => "rs1, rs2, label",
            Operands::RdUpper => "rd, imm",
            Operands::RdTarget => "rd, label",
            Operands::Fence => "[pred, succ]",
            Operands::None => "",
        }
    }
}

/// Campos fijos que identifican la instrucción. Los `None` no se comprueban al decodificar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Encoding {
    pub opcode: u8,
    pub funct3: Option<u8>,
    /// Bits 31..25: `funct7` en el formato R y en los desplazamientos con inmediato.
    pub funct7: Option<u8>,
    /// Bits 31..20 de `ecall` y `ebreak`, que además exigen `rd` y `rs1` a cero.
    pub funct12: Option<u16>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct InstructionSpec {
    pub mnemonic: Mnemonic,
    pub name: &'static str,
    pub extension: Extension,
    pub group: Group,
    pub format: Format,
    pub operands: Operands,
    pub encoding: Encoding,
    /// Qué hace, en una frase.
    pub summary: &'static str,
    /// Operación en pseudocódigo compacto.
    pub operation: &'static str,
}

impl InstructionSpec {
    /// Sintaxis completa, p. ej. `addi rd, rs1, imm`.
    pub fn syntax(&self) -> String {
        let operands = self.operands.syntax();
        if operands.is_empty() {
            self.name.to_owned()
        } else {
            format!("{} {operands}", self.name)
        }
    }
}

impl Mnemonic {
    pub fn spec(self) -> &'static InstructionSpec {
        &INSTRUCTIONS[self as usize]
    }

    pub fn name(self) -> &'static str {
        self.spec().name
    }
}

/// Busca una instrucción real por su nombre, sin distinguir mayúsculas.
pub fn find(name: &str) -> Option<&'static InstructionSpec> {
    INSTRUCTIONS
        .iter()
        .find(|spec| spec.name.eq_ignore_ascii_case(name))
}

const OP: u8 = 0b011_0011;
const OP_IMM: u8 = 0b001_0011;
const LOAD: u8 = 0b000_0011;
const STORE: u8 = 0b010_0011;
const BRANCH: u8 = 0b110_0011;
const MISC_MEM: u8 = 0b000_1111;
const SYSTEM: u8 = 0b111_0011;

const fn enc(opcode: u8, funct3: Option<u8>, funct7: Option<u8>, funct12: Option<u16>) -> Encoding {
    Encoding {
        opcode,
        funct3,
        funct7,
        funct12,
    }
}

#[allow(clippy::too_many_arguments)]
const fn spec(
    mnemonic: Mnemonic,
    name: &'static str,
    extension: Extension,
    group: Group,
    format: Format,
    operands: Operands,
    encoding: Encoding,
    summary: &'static str,
    operation: &'static str,
) -> InstructionSpec {
    InstructionSpec {
        mnemonic,
        name,
        extension,
        group,
        format,
        operands,
        encoding,
        summary,
        operation,
    }
}

const fn r(
    mnemonic: Mnemonic,
    name: &'static str,
    group: Group,
    f3: u8,
    f7: u8,
    summary: &'static str,
    operation: &'static str,
) -> InstructionSpec {
    let extension = if f7 == 1 { Extension::M } else { Extension::I };
    spec(
        mnemonic,
        name,
        extension,
        group,
        Format::R,
        Operands::RdRs1Rs2,
        enc(OP, Some(f3), Some(f7), None),
        summary,
        operation,
    )
}

const fn imm(
    mnemonic: Mnemonic,
    name: &'static str,
    group: Group,
    f3: u8,
    summary: &'static str,
    operation: &'static str,
) -> InstructionSpec {
    spec(
        mnemonic,
        name,
        Extension::I,
        group,
        Format::I,
        Operands::RdRs1Imm,
        enc(OP_IMM, Some(f3), None, None),
        summary,
        operation,
    )
}

const fn shift(
    mnemonic: Mnemonic,
    name: &'static str,
    f3: u8,
    f7: u8,
    summary: &'static str,
    operation: &'static str,
) -> InstructionSpec {
    spec(
        mnemonic,
        name,
        Extension::I,
        Group::Shift,
        Format::I,
        Operands::RdRs1Shamt,
        enc(OP_IMM, Some(f3), Some(f7), None),
        summary,
        operation,
    )
}

const fn load(
    mnemonic: Mnemonic,
    name: &'static str,
    f3: u8,
    summary: &'static str,
    operation: &'static str,
) -> InstructionSpec {
    spec(
        mnemonic,
        name,
        Extension::I,
        Group::Load,
        Format::I,
        Operands::RdOffsetRs1,
        enc(LOAD, Some(f3), None, None),
        summary,
        operation,
    )
}

const fn store(
    mnemonic: Mnemonic,
    name: &'static str,
    f3: u8,
    summary: &'static str,
    operation: &'static str,
) -> InstructionSpec {
    spec(
        mnemonic,
        name,
        Extension::I,
        Group::Store,
        Format::S,
        Operands::Rs2OffsetRs1,
        enc(STORE, Some(f3), None, None),
        summary,
        operation,
    )
}

const fn branch(
    mnemonic: Mnemonic,
    name: &'static str,
    f3: u8,
    summary: &'static str,
    operation: &'static str,
) -> InstructionSpec {
    spec(
        mnemonic,
        name,
        Extension::I,
        Group::Branch,
        Format::B,
        Operands::Rs1Rs2Target,
        enc(BRANCH, Some(f3), None, None),
        summary,
        operation,
    )
}

use Group as G;
use Mnemonic as M;

/// Todas las instrucciones reales admitidas, en el orden de [`Mnemonic`].
#[rustfmt::skip]
pub static INSTRUCTIONS: [InstructionSpec; 48] = [
    spec(M::Lui, "lui", Extension::I, G::Upper, Format::U, Operands::RdUpper, enc(0b011_0111, None, None, None),
        "Carga los 20 bits altos de rd con el inmediato y pone a cero los 12 bajos", "rd = imm << 12"),
    spec(M::Auipc, "auipc", Extension::I, G::Upper, Format::U, Operands::RdUpper, enc(0b001_0111, None, None, None),
        "Suma al PC el inmediato desplazado 12 bits", "rd = pc + (imm << 12)"),
    spec(M::Jal, "jal", Extension::I, G::Jump, Format::J, Operands::RdTarget, enc(0b110_1111, None, None, None),
        "Salta a la etiqueta y guarda en rd la dirección de vuelta", "rd = pc + 4; pc = pc + offset"),
    spec(M::Jalr, "jalr", Extension::I, G::Jump, Format::I, Operands::RdOffsetRs1, enc(0b110_0111, Some(0), None, None),
        "Salta a rs1 + offset (con el bit 0 a cero) y guarda en rd la dirección de vuelta", "t = pc + 4; pc = (rs1 + offset) & ~1; rd = t"),
    branch(M::Beq, "beq", 0b000, "Salta si rs1 y rs2 son iguales", "if (rs1 == rs2) pc = pc + offset"),
    branch(M::Bne, "bne", 0b001, "Salta si rs1 y rs2 son distintos", "if (rs1 != rs2) pc = pc + offset"),
    branch(M::Blt, "blt", 0b100, "Salta si rs1 < rs2 con signo", "if (rs1 <s rs2) pc = pc + offset"),
    branch(M::Bge, "bge", 0b101, "Salta si rs1 >= rs2 con signo", "if (rs1 >=s rs2) pc = pc + offset"),
    branch(M::Bltu, "bltu", 0b110, "Salta si rs1 < rs2 sin signo", "if (rs1 <u rs2) pc = pc + offset"),
    branch(M::Bgeu, "bgeu", 0b111, "Salta si rs1 >= rs2 sin signo", "if (rs1 >=u rs2) pc = pc + offset"),
    load(M::Lb, "lb", 0b000, "Carga un byte con extensión de signo", "rd = sext(mem8[rs1 + offset])"),
    load(M::Lh, "lh", 0b001, "Carga media palabra (16 bits) con extensión de signo", "rd = sext(mem16[rs1 + offset])"),
    load(M::Lw, "lw", 0b010, "Carga una palabra (32 bits)", "rd = mem32[rs1 + offset]"),
    load(M::Lbu, "lbu", 0b100, "Carga un byte sin signo (rellena con ceros)", "rd = zext(mem8[rs1 + offset])"),
    load(M::Lhu, "lhu", 0b101, "Carga media palabra sin signo (rellena con ceros)", "rd = zext(mem16[rs1 + offset])"),
    store(M::Sb, "sb", 0b000, "Guarda el byte bajo de rs2", "mem8[rs1 + offset] = rs2[7:0]"),
    store(M::Sh, "sh", 0b001, "Guarda la media palabra baja de rs2", "mem16[rs1 + offset] = rs2[15:0]"),
    store(M::Sw, "sw", 0b010, "Guarda la palabra rs2", "mem32[rs1 + offset] = rs2"),
    imm(M::Addi, "addi", G::Arithmetic, 0b000, "Suma un inmediato", "rd = rs1 + imm"),
    imm(M::Slti, "slti", G::Compare, 0b010, "Pone rd a 1 si rs1 < imm con signo, si no a 0", "rd = (rs1 <s imm) ? 1 : 0"),
    imm(M::Sltiu, "sltiu", G::Compare, 0b011, "Pone rd a 1 si rs1 < imm sin signo, si no a 0", "rd = (rs1 <u imm) ? 1 : 0"),
    imm(M::Xori, "xori", G::Logical, 0b100, "O exclusiva bit a bit con un inmediato", "rd = rs1 ^ imm"),
    imm(M::Ori, "ori", G::Logical, 0b110, "O bit a bit con un inmediato", "rd = rs1 | imm"),
    imm(M::Andi, "andi", G::Logical, 0b111, "Y bit a bit con un inmediato", "rd = rs1 & imm"),
    shift(M::Slli, "slli", 0b001, 0b000_0000, "Desplaza a la izquierda un número fijo de bits", "rd = rs1 << shamt"),
    shift(M::Srli, "srli", 0b101, 0b000_0000, "Desplaza a la derecha rellenando con ceros", "rd = rs1 >>u shamt"),
    shift(M::Srai, "srai", 0b101, 0b010_0000, "Desplaza a la derecha conservando el signo", "rd = rs1 >>s shamt"),
    r(M::Add, "add", G::Arithmetic, 0b000, 0b000_0000, "Suma", "rd = rs1 + rs2"),
    r(M::Sub, "sub", G::Arithmetic, 0b000, 0b010_0000, "Resta", "rd = rs1 - rs2"),
    r(M::Sll, "sll", G::Shift, 0b001, 0b000_0000, "Desplaza a la izquierda tantos bits como indique rs2[4:0]", "rd = rs1 << rs2[4:0]"),
    r(M::Slt, "slt", G::Compare, 0b010, 0b000_0000, "Pone rd a 1 si rs1 < rs2 con signo, si no a 0", "rd = (rs1 <s rs2) ? 1 : 0"),
    r(M::Sltu, "sltu", G::Compare, 0b011, 0b000_0000, "Pone rd a 1 si rs1 < rs2 sin signo, si no a 0", "rd = (rs1 <u rs2) ? 1 : 0"),
    r(M::Xor, "xor", G::Logical, 0b100, 0b000_0000, "O exclusiva bit a bit", "rd = rs1 ^ rs2"),
    r(M::Srl, "srl", G::Shift, 0b101, 0b000_0000, "Desplaza a la derecha rellenando con ceros", "rd = rs1 >>u rs2[4:0]"),
    r(M::Sra, "sra", G::Shift, 0b101, 0b010_0000, "Desplaza a la derecha conservando el signo", "rd = rs1 >>s rs2[4:0]"),
    r(M::Or, "or", G::Logical, 0b110, 0b000_0000, "O bit a bit", "rd = rs1 | rs2"),
    r(M::And, "and", G::Logical, 0b111, 0b000_0000, "Y bit a bit", "rd = rs1 & rs2"),
    spec(M::Fence, "fence", Extension::I, G::System, Format::I, Operands::Fence, enc(MISC_MEM, Some(0), None, None),
        "Ordena los accesos a memoria y E/S anteriores respecto a los posteriores", "barrera de memoria (sin efecto en un solo núcleo)"),
    spec(M::Ecall, "ecall", Extension::I, G::System, Format::I, Operands::None, enc(SYSTEM, Some(0), None, Some(0)),
        "Llamada al sistema: el servicio se elige con a7 y los argumentos van en a0..a6", "servicio[a7](a0, a1, ...)"),
    spec(M::Ebreak, "ebreak", Extension::I, G::System, Format::I, Operands::None, enc(SYSTEM, Some(0), None, Some(1)),
        "Detiene la ejecución para el depurador", "parada"),
    r(M::Mul, "mul", G::Multiply, 0b000, 0b000_0001, "Multiplica y se queda con los 32 bits bajos", "rd = (rs1 * rs2)[31:0]"),
    r(M::Mulh, "mulh", G::Multiply, 0b001, 0b000_0001, "32 bits altos del producto con signo × con signo", "rd = (rs1 *s rs2)[63:32]"),
    r(M::Mulhsu, "mulhsu", G::Multiply, 0b010, 0b000_0001, "32 bits altos del producto con signo × sin signo", "rd = (rs1 *s rs2 u)[63:32]"),
    r(M::Mulhu, "mulhu", G::Multiply, 0b011, 0b000_0001, "32 bits altos del producto sin signo × sin signo", "rd = (rs1 *u rs2)[63:32]"),
    r(M::Div, "div", G::Divide, 0b100, 0b000_0001, "Cociente con signo, redondeando hacia cero (entre 0 da −1)", "rd = rs1 /s rs2"),
    r(M::Divu, "divu", G::Divide, 0b101, 0b000_0001, "Cociente sin signo (entre 0 da 0xFFFFFFFF)", "rd = rs1 /u rs2"),
    r(M::Rem, "rem", G::Divide, 0b110, 0b000_0001, "Resto con signo; tiene el signo del dividendo (entre 0 da rs1)", "rd = rs1 %s rs2"),
    r(M::Remu, "remu", G::Divide, 0b111, 0b000_0001, "Resto sin signo (entre 0 da rs1)", "rd = rs1 %u rs2"),
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn table_order_matches_mnemonic_enum() {
        for (position, spec) in INSTRUCTIONS.iter().enumerate() {
            assert_eq!(spec.mnemonic as usize, position, "{}", spec.name);
        }
    }

    #[test]
    fn names_are_unique_and_lowercase() {
        let mut seen = HashSet::new();
        for spec in &INSTRUCTIONS {
            assert!(seen.insert(spec.name), "repetida: {}", spec.name);
            assert_eq!(spec.name, spec.name.to_ascii_lowercase());
            assert_eq!(
                find(&spec.name.to_ascii_uppercase()).map(|s| s.mnemonic),
                Some(spec.mnemonic)
            );
        }
    }

    #[test]
    fn encodings_do_not_overlap() {
        let mut seen = HashSet::new();
        for spec in &INSTRUCTIONS {
            let key = (
                spec.encoding.opcode,
                spec.encoding.funct3,
                spec.encoding.funct7,
                spec.encoding.funct12,
            );
            assert!(seen.insert(key), "codificación repetida: {}", spec.name);
        }
    }

    #[test]
    fn every_group_has_instructions() {
        for group in Group::ALL {
            assert!(
                INSTRUCTIONS.iter().any(|spec| spec.group == group),
                "{group:?}"
            );
        }
    }

    #[test]
    fn syntax_is_built_from_operands() {
        assert_eq!(Mnemonic::Addi.spec().syntax(), "addi rd, rs1, imm");
        assert_eq!(Mnemonic::Sw.spec().syntax(), "sw rs2, offset(rs1)");
        assert_eq!(Mnemonic::Ecall.spec().syntax(), "ecall");
    }
}
