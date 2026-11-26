//! Lo que ha hecho cada etapa de un procesador en el último ciclo, igual para todos los modelos:
//! qué instrucción había en cada etapa y vía, si esperaba o se ha vaciado, las señales de control
//! y los valores de los cables. La interfaz pinta la ruta de datos y la tabla del pipeline con
//! esto, sin saber cómo está hecho el modelo por dentro.

use rv_isa::{Instruction, Reg};
use serde::Serialize;

use crate::exec::{AluSource, Operation};
use crate::pipeline::Bypass;
use crate::processor::{DataAccess, RegisterWrite, Retired, Trap};

/// Etapa del procesador. El de un ciclo tiene una sola, `Single`, que lo hace todo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Single,
    If,
    Id,
    Ex,
    Mem,
    Wb,
}

impl Stage {
    /// Etapas de los modelos segmentados, en el orden en que las recorre una instrucción.
    pub const PIPELINE: [Stage; 5] = [Stage::If, Stage::Id, Stage::Ex, Stage::Mem, Stage::Wb];

    pub fn name(self) -> &'static str {
        match self {
            Stage::Single => "CPU",
            Stage::If => "IF",
            Stage::Id => "ID",
            Stage::Ex => "EX",
            Stage::Mem => "MEM",
            Stage::Wb => "WB",
        }
    }

    /// Cierto si en esta etapa la instrucción ya ha pasado por la ALU.
    fn executed(self) -> bool {
        matches!(self, Stage::Single | Stage::Ex | Stage::Mem | Stage::Wb)
    }
}

/// Qué se escribe en `rd` en WB.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WriteSource {
    /// Resultado de la ALU (también `lui` y `auipc`).
    Alu,
    /// Lo leído de memoria.
    Memory,
    /// Dirección de retorno, `pc + 4` (`jal` y `jalr`).
    Link,
}

/// Señales de la unidad de control para una instrucción ya decodificada.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Control {
    pub operation: Operation,
    pub reads_rs1: bool,
    pub reads_rs2: bool,
    /// Segundo operando de la ALU; `None` si la instrucción no la usa.
    pub alu_source: Option<AluSource>,
    pub mem_read: bool,
    pub mem_write: bool,
    pub branch: bool,
    pub jump: bool,
    /// De dónde sale lo que se escribe en `rd`; `None` si no escribe registro o `rd` es `x0`.
    pub write_source: Option<WriteSource>,
}

impl Control {
    pub fn of(instruction: &Instruction) -> Control {
        let operation = Operation::of(instruction.mnemonic);
        let (reads_rs1, reads_rs2) = match operation {
            Operation::Alu(_, AluSource::Register) | Operation::Branch(_) | Operation::Store(_) => {
                (true, true)
            }
            Operation::Alu(_, AluSource::Immediate) | Operation::Jalr | Operation::Load(_) => {
                (true, false)
            }
            Operation::Lui
            | Operation::Auipc
            | Operation::Jal
            | Operation::Fence
            | Operation::Ecall
            | Operation::Ebreak => (false, false),
        };
        let alu_source = match operation {
            Operation::Alu(_, source) => Some(source),
            Operation::Branch(_) => Some(AluSource::Register),
            Operation::Auipc | Operation::Jalr | Operation::Load(_) | Operation::Store(_) => {
                Some(AluSource::Immediate)
            }
            Operation::Lui
            | Operation::Jal
            | Operation::Fence
            | Operation::Ecall
            | Operation::Ebreak => None,
        };
        let write_source = match operation {
            Operation::Alu(..) | Operation::Lui | Operation::Auipc => Some(WriteSource::Alu),
            Operation::Load(_) => Some(WriteSource::Memory),
            Operation::Jal | Operation::Jalr => Some(WriteSource::Link),
            Operation::Branch(_)
            | Operation::Store(_)
            | Operation::Fence
            | Operation::Ecall
            | Operation::Ebreak => None,
        };
        Control {
            operation,
            reads_rs1,
            reads_rs2,
            alu_source,
            mem_read: matches!(operation, Operation::Load(_)),
            mem_write: matches!(operation, Operation::Store(_)),
            branch: matches!(operation, Operation::Branch(_)),
            jump: matches!(operation, Operation::Jal | Operation::Jalr),
            write_source: write_source.filter(|_| instruction.rd != Reg::ZERO),
        }
    }
}

/// Operando de la instrucción: el registro, el valor que se usa y, en EX, si llegó adelantado.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Operand {
    pub reg: Reg,
    pub value: u32,
    pub bypass: Option<Bypass>,
}

/// Qué ha pasado con la instrucción de un hueco en este ciclo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SlotState {
    /// Ha hecho su trabajo y sigue adelante.
    Active,
    /// Se queda donde está: la unidad de riesgos la retiene, o espera a una `ecall` o a su
    /// pareja en la doble emisión.
    Stalled,
    /// Un salto la ha descartado: iba por el camino equivocado.
    Flushed,
}

/// Instrucción que ha ocupado un hueco de una etapa en el último ciclo. Los valores que la
/// instrucción aún no tiene en esa etapa valen `None` (en ID todavía no hay resultado).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Slot {
    pub pc: u32,
    /// `None` si la búsqueda falló o la instrucción viaja ya como un fallo sin su palabra.
    pub word: Option<u32>,
    pub instruction: Option<Instruction>,
    pub state: SlotState,
    /// Solo a partir de ID: IF aún no ha decodificado.
    pub control: Option<Control>,
    pub rs1: Option<Operand>,
    pub rs2: Option<Operand>,
    /// Lo que se escribirá en `rd`: salida de la ALU, enlace o, tras MEM, lo cargado.
    pub result: Option<u32>,
    pub next_pc: Option<u32>,
    pub address: Option<u32>,
    pub data_access: Option<DataAccess>,
    /// Solo en la etapa donde se retira.
    pub register_write: Option<RegisterWrite>,
    /// Excepción que lleva la instrucción o que la para en este ciclo.
    pub trap: Option<Trap>,
}

impl Slot {
    /// Hueco con solo lo que se sabe de una instrucción recién buscada.
    pub(crate) fn fetched(pc: u32, word: u32, state: SlotState) -> Slot {
        Slot {
            word: Some(word),
            instruction: Instruction::decode(word).ok(),
            ..Slot::bare(pc, state)
        }
    }

    /// Instrucción que viaja como un fallo: solo se conservan su `pc` y la excepción.
    pub(crate) fn fault(pc: u32, trap: Trap, state: SlotState) -> Slot {
        Slot {
            trap: Some(trap),
            ..Slot::bare(pc, state)
        }
    }

    /// Instrucción decodificada en `stage` con los operandos que lleva y lo calculado hasta
    /// esa etapa. `bypasses` solo cuenta en EX, que es donde se adelanta.
    pub(crate) fn decoded(
        stage: Stage,
        state: SlotState,
        progress: Progress,
        bypasses: [Option<Bypass>; 2],
    ) -> Slot {
        let instruction = progress.instruction;
        let control = Control::of(&instruction);
        let bypasses = if stage == Stage::Ex {
            bypasses
        } else {
            [None; 2]
        };
        let operand = |reads: bool, reg: Reg, value: u32, bypass: Option<Bypass>| {
            reads.then_some(Operand { reg, value, bypass })
        };
        let executed = stage.executed();
        Slot {
            word: Some(progress.word),
            instruction: Some(instruction),
            control: Some(control),
            rs1: operand(
                control.reads_rs1,
                instruction.rs1,
                progress.operands[0],
                bypasses[0],
            ),
            rs2: operand(
                control.reads_rs2,
                instruction.rs2,
                progress.operands[1],
                bypasses[1],
            ),
            result: progress.result.filter(|_| executed),
            next_pc: executed.then_some(progress.next_pc),
            address: progress.address.filter(|_| executed),
            data_access: progress.data_access,
            ..Slot::bare(progress.pc, state)
        }
    }

    fn bare(pc: u32, state: SlotState) -> Slot {
        Slot {
            pc,
            word: None,
            instruction: None,
            state,
            control: None,
            rs1: None,
            rs2: None,
            result: None,
            next_pc: None,
            address: None,
            data_access: None,
            register_write: None,
            trap: None,
        }
    }

    /// Completa el hueco de la etapa de retirada con lo que ha escrito.
    pub(crate) fn retire(&mut self, retired: &Retired) {
        self.register_write = retired.register_write;
        self.data_access = retired.data_access;
    }
}

/// Lo que se sabe de una instrucción decodificada hasta su etapa actual.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Progress {
    pub(crate) pc: u32,
    pub(crate) word: u32,
    pub(crate) instruction: Instruction,
    /// `rs1` y `rs2`: leídos en ID y, desde EX, ya adelantados.
    pub(crate) operands: [u32; 2],
    pub(crate) next_pc: u32,
    pub(crate) result: Option<u32>,
    pub(crate) address: Option<u32>,
    pub(crate) data_access: Option<DataAccess>,
}

/// Una etapa con un hueco por vía; `None` es una burbuja.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StageView {
    pub stage: Stage,
    pub lanes: Vec<Option<Slot>>,
}

/// Instantánea del último ciclo: `cycle` es el número de ciclos contados hasta él (0 antes de
/// empezar) y `stages` va de la primera etapa a la última.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PipelineView {
    pub cycle: u64,
    pub stages: Vec<StageView>,
}

impl PipelineView {
    /// Vista sin nada dentro, con `lanes` huecos en cada etapa.
    pub(crate) fn empty(stages: &[Stage], lanes: usize) -> PipelineView {
        PipelineView {
            cycle: 0,
            stages: stages
                .iter()
                .map(|&stage| StageView {
                    stage,
                    lanes: vec![None; lanes],
                })
                .collect(),
        }
    }

    pub fn stage(&self, stage: Stage) -> Option<&StageView> {
        self.stages.iter().find(|view| view.stage == stage)
    }

    /// Todos los huecos ocupados, con su etapa y su vía.
    pub fn slots(&self) -> impl Iterator<Item = (Stage, usize, &Slot)> {
        self.stages.iter().flat_map(|view| {
            view.lanes
                .iter()
                .enumerate()
                .filter_map(move |(lane, slot)| slot.as_ref().map(|slot| (view.stage, lane, slot)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::AluOp;

    fn instruction(source: &str) -> Instruction {
        rv_asm::assemble(source).unwrap().listing[0].instruction
    }

    #[test]
    fn control_signals_follow_the_instruction_class() {
        let add = Control::of(&instruction("add a0, a1, a2"));
        assert_eq!(
            add,
            Control {
                operation: Operation::Alu(AluOp::Add, AluSource::Register),
                reads_rs1: true,
                reads_rs2: true,
                alu_source: Some(AluSource::Register),
                mem_read: false,
                mem_write: false,
                branch: false,
                jump: false,
                write_source: Some(WriteSource::Alu),
            }
        );
        let load = Control::of(&instruction("lw a0, 4(sp)"));
        assert!(load.mem_read && load.reads_rs1 && !load.reads_rs2);
        assert_eq!(load.write_source, Some(WriteSource::Memory));
        let store = Control::of(&instruction("sw a0, 4(sp)"));
        assert!(store.mem_write && store.reads_rs2 && store.write_source.is_none());
        let branch = Control::of(&instruction("beq a0, a1, 0"));
        assert!(branch.branch && !branch.jump && branch.write_source.is_none());
        let call = Control::of(&instruction("jal ra, 0"));
        assert!(call.jump && !call.reads_rs1);
        assert_eq!(call.write_source, Some(WriteSource::Link));
        let lui = Control::of(&instruction("lui a0, 1"));
        assert!(!lui.reads_rs1 && lui.alu_source.is_none());
    }

    #[test]
    fn writing_x0_is_not_a_register_write() {
        assert_eq!(
            Control::of(&instruction("addi x0, x0, 0")).write_source,
            None
        );
        assert_eq!(Control::of(&instruction("jal x0, 0")).write_source, None);
    }

    #[test]
    fn values_appear_only_once_computed() {
        let addi = instruction("addi a0, a1, 3");
        let progress = Progress {
            pc: 0,
            word: 0,
            instruction: addi,
            operands: [5, 0],
            next_pc: 4,
            result: Some(8),
            address: None,
            data_access: None,
        };
        let bypass = [Some(Bypass::ExMem), None];
        let id = Slot::decoded(Stage::Id, SlotState::Active, progress, bypass);
        assert_eq!((id.result, id.next_pc), (None, None));
        assert_eq!(id.rs1.map(|operand| operand.bypass), Some(None));
        assert_eq!(id.rs2, None);
        let ex = Slot::decoded(Stage::Ex, SlotState::Active, progress, bypass);
        assert_eq!((ex.result, ex.next_pc), (Some(8), Some(4)));
        assert_eq!(
            ex.rs1,
            Some(Operand {
                reg: addi.rs1,
                value: 5,
                bypass: Some(Bypass::ExMem)
            })
        );
    }
}
