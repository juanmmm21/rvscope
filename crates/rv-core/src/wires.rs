//! Valores de los cables de la ruta de datos para una instrucción, sacados de lo que el hueco de
//! la vista ya sabe de ella. Así la interfaz dibuja qué entra en la ALU, qué sale del sumador de
//! saltos o por dónde va el siguiente `pc` sin tener que decidir nada del procesador.
//!
//! La ruta de datos que se describe es la de rvscope: la ALU calcula operaciones, direcciones
//! de memoria y el destino de `jalr`, y compara en los saltos condicionales; un sumador aparte
//! calcula `pc + inmediato` para `jal` y los saltos condicionales.

use serde::Serialize;

use crate::exec::{AluOp, AluSource, BranchCond, Operation};
use crate::view::Slot;

/// Primer operando de la ALU.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AluInput {
    Register,
    Pc,
    /// Constante 0 (`lui` pasa el inmediato por la ALU sumándole 0).
    Zero,
}

/// Lo que hace la ALU: una operación o, en un salto condicional, la comparación.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum AluFunction {
    Arithmetic(AluOp),
    Compare(BranchCond),
}

/// Entradas y salida de la ALU. En una comparación, `out` vale 1 si el salto se toma.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct AluWires {
    pub function: AluFunction,
    pub a_source: AluInput,
    pub b_source: AluSource,
    pub a: u32,
    pub b: u32,
    pub out: u32,
}

/// De dónde sale el siguiente `pc`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PcSource {
    /// `pc + 4`.
    Sequential,
    /// Sumador de saltos, `pc + inmediato` (`jal` y saltos tomados).
    Target,
    /// Salida de la ALU con el bit bajo a 0 (`jalr`).
    Alu,
}

/// Cables de una instrucción decodificada. Los que dependen de la ejecución valen `None`
/// mientras la instrucción no ha pasado por la ALU (en IF e ID de los segmentados).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Wires {
    /// `pc + 4`: lo que sigue si no hay salto y el enlace de `jal`/`jalr`.
    pub pc_plus_4: u32,
    /// Salida del generador de inmediatos; `None` si la instrucción no lleva inmediato.
    pub immediate: Option<u32>,
    pub alu: Option<AluWires>,
    /// Salida del sumador de saltos, solo en `jal` y saltos condicionales.
    pub target: Option<u32>,
    pub pc_source: Option<PcSource>,
}

impl Wires {
    /// Cables de lo que hay en `slot`; `None` si la instrucción no está decodificada (IF, o un
    /// fallo que ya no conserva la instrucción).
    pub fn of(slot: &Slot) -> Option<Wires> {
        let instruction = slot.instruction?;
        let control = slot.control?;
        let operation = control.operation;
        let pc = slot.pc;
        let imm = instruction.imm as u32;
        let immediate = match operation {
            Operation::Alu(_, AluSource::Register)
            | Operation::Fence
            | Operation::Ecall
            | Operation::Ebreak => None,
            _ => Some(imm),
        };
        let executed = slot.next_pc.is_some();
        let rs1 = slot.rs1.map(|operand| operand.value);
        let rs2 = slot.rs2.map(|operand| operand.value);
        let alu = executed
            .then(|| alu_wires(operation, pc, imm, rs1, rs2))
            .flatten();
        let target = match operation {
            Operation::Jal | Operation::Branch(_) if executed => Some(pc.wrapping_add(imm)),
            _ => None,
        };
        let pc_source = executed.then(|| match operation {
            Operation::Jal => PcSource::Target,
            Operation::Jalr => PcSource::Alu,
            Operation::Branch(_) if alu.is_some_and(|alu| alu.out == 1) => PcSource::Target,
            _ => PcSource::Sequential,
        });
        Some(Wires {
            pc_plus_4: pc.wrapping_add(4),
            immediate,
            alu,
            target,
            pc_source,
        })
    }
}

fn alu_wires(
    operation: Operation,
    pc: u32,
    imm: u32,
    rs1: Option<u32>,
    rs2: Option<u32>,
) -> Option<AluWires> {
    let arithmetic = |op: AluOp, a_source, a, b_source, b| AluWires {
        function: AluFunction::Arithmetic(op),
        a_source,
        b_source,
        a,
        b,
        out: op.apply(a, b),
    };
    let wires = match operation {
        Operation::Alu(op, AluSource::Register) => {
            arithmetic(op, AluInput::Register, rs1?, AluSource::Register, rs2?)
        }
        Operation::Alu(op, AluSource::Immediate) => {
            arithmetic(op, AluInput::Register, rs1?, AluSource::Immediate, imm)
        }
        Operation::Jalr | Operation::Load(_) | Operation::Store(_) => arithmetic(
            AluOp::Add,
            AluInput::Register,
            rs1?,
            AluSource::Immediate,
            imm,
        ),
        Operation::Lui => arithmetic(AluOp::Add, AluInput::Zero, 0, AluSource::Immediate, imm),
        Operation::Auipc => arithmetic(AluOp::Add, AluInput::Pc, pc, AluSource::Immediate, imm),
        Operation::Branch(cond) => {
            let (a, b) = (rs1?, rs2?);
            AluWires {
                function: AluFunction::Compare(cond),
                a_source: AluInput::Register,
                b_source: AluSource::Register,
                a,
                b,
                out: u32::from(cond.taken(a, b)),
            }
        }
        Operation::Jal | Operation::Fence | Operation::Ecall | Operation::Ebreak => return None,
    };
    Some(wires)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::{Progress, SlotState, Stage};
    use rv_isa::Instruction;

    fn slot(source: &str, stage: Stage, operands: [u32; 2], next_pc: u32) -> Slot {
        let instruction: Instruction = rv_asm::assemble(source).unwrap().listing[0].instruction;
        let progress = Progress {
            pc: 0x100,
            word: instruction.encode().unwrap(),
            instruction,
            operands,
            next_pc,
            result: None,
            address: None,
            data_access: None,
        };
        Slot::decoded(stage, SlotState::Active, progress, [None; 2])
    }

    #[test]
    fn register_operations_have_no_immediate() {
        let wires = Wires::of(&slot("sub a0, a1, a2", Stage::Single, [9, 4], 0x104)).unwrap();
        assert_eq!(wires.immediate, None);
        let alu = wires.alu.unwrap();
        assert_eq!((alu.a, alu.b, alu.out), (9, 4, 5));
        assert_eq!(alu.function, AluFunction::Arithmetic(AluOp::Sub));
        assert_eq!(wires.pc_source, Some(PcSource::Sequential));
        assert_eq!(wires.target, None);
    }

    #[test]
    fn upper_immediates_go_through_the_alu() {
        let lui = Wires::of(&slot("lui a0, 2", Stage::Single, [0, 0], 0x104)).unwrap();
        let alu = lui.alu.unwrap();
        assert_eq!((alu.a_source, alu.out), (AluInput::Zero, 0x2000));
        let auipc = Wires::of(&slot("auipc a0, 1", Stage::Single, [0, 0], 0x104)).unwrap();
        let alu = auipc.alu.unwrap();
        assert_eq!(
            (alu.a_source, alu.a, alu.out),
            (AluInput::Pc, 0x100, 0x1100)
        );
    }

    #[test]
    fn branches_compare_in_the_alu_and_add_the_target_apart() {
        let taken = Wires::of(&slot("blt a0, a1, 8", Stage::Ex, [u32::MAX, 1], 0x108)).unwrap();
        let alu = taken.alu.unwrap();
        assert_eq!(alu.function, AluFunction::Compare(BranchCond::Lt));
        assert_eq!(alu.out, 1);
        assert_eq!(taken.target, Some(0x108));
        assert_eq!(taken.pc_source, Some(PcSource::Target));
        let not_taken =
            Wires::of(&slot("bltu a0, a1, 8", Stage::Ex, [u32::MAX, 1], 0x104)).unwrap();
        assert_eq!(not_taken.alu.unwrap().out, 0);
        assert_eq!(not_taken.pc_source, Some(PcSource::Sequential));
    }

    #[test]
    fn jumps_pick_their_next_pc() {
        let jal = Wires::of(&slot("jal ra, 16", Stage::Single, [0, 0], 0x110)).unwrap();
        assert_eq!((jal.alu, jal.target), (None, Some(0x110)));
        assert_eq!(jal.pc_source, Some(PcSource::Target));
        assert_eq!(jal.pc_plus_4, 0x104);
        let jalr = Wires::of(&slot("jalr ra, 3(a0)", Stage::Single, [0x200, 0], 0x204)).unwrap();
        assert_eq!(jalr.alu.unwrap().out, 0x203);
        assert_eq!(jalr.pc_source, Some(PcSource::Alu));
    }

    #[test]
    fn before_execution_only_the_immediate_is_known() {
        let wires = Wires::of(&slot("addi a0, a1, -1", Stage::Id, [5, 0], 0x104)).unwrap();
        assert_eq!(wires.immediate, Some(u32::MAX));
        assert_eq!((wires.alu, wires.pc_source), (None, None));
    }

    #[test]
    fn system_instructions_use_no_alu() {
        let wires = Wires::of(&slot("ecall", Stage::Single, [0, 0], 0x104)).unwrap();
        assert_eq!((wires.immediate, wires.alu), (None, None));
        assert_eq!(wires.pc_source, Some(PcSource::Sequential));
    }
}
