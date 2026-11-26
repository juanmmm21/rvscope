//! Piezas comunes de los modelos segmentados: registros de segmentación, lo que lleva cada
//! instrucción entre etapas y el trabajo de ID, MEM y WB, que no cambia entre modelos. Lo que
//! sí cambia (cuántas vías, la unidad de riesgos) vive en cada modelo.

use rv_isa::{Instruction, Reg};
use serde::Serialize;

use crate::exec::{Operation, access_memory};
use crate::memory::Memory;
use crate::processor::{DataAccess, Halt, RegisterWrite, Retired, Trap};
use crate::state::{ArchState, Registers};
use crate::syscall::{EcallEffect, ecall};
use crate::view::{Progress, Slot, SlotState, Stage};

/// Registro de segmentación del que EX ha tomado un operando en vez del valor leído en ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Bypass {
    /// Resultado de la ALU de una instrucción de justo delante, que ahora está en MEM.
    ExMem,
    /// Lo que escribe una de dos ciclos por delante, que ahora está en WB: su resultado o lo
    /// cargado.
    MemWb,
}

/// Registro de segmentación entre dos etapas: vacío (burbuja), con una instrucción o con una
/// instrucción que ya ha fallado y viaja hasta WB para parar allí.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Latch<T> {
    Bubble,
    Holds(T),
    Fault(Halt),
}

/// IF/ID: la palabra buscada, sin decodificar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fetched {
    pub(crate) pc: u32,
    pub(crate) word: u32,
}

/// Instrucción decodificada que avanza por ID/EX, EX/MEM y MEM/WB. Cada etapa rellena lo suyo:
/// ID los operandos, EX `next_pc`, el resultado y la dirección, y MEM el valor cargado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Uop {
    pub(crate) pc: u32,
    pub(crate) word: u32,
    pub(crate) instruction: Instruction,
    pub(crate) operation: Operation,
    pub(crate) rs1: u32,
    pub(crate) rs2: u32,
    pub(crate) next_pc: u32,
    pub(crate) result: Option<u32>,
    pub(crate) address: Option<u32>,
    pub(crate) data_access: Option<DataAccess>,
}

impl Latch<Fetched> {
    /// Hueco de IF con la palabra buscada.
    pub(crate) fn slot(&self, state: SlotState) -> Option<Slot> {
        match self {
            Latch::Bubble => None,
            Latch::Fault(halt) => Some(Slot::fault(halt.pc, halt.trap, state)),
            Latch::Holds(fetched) => Some(Slot::fetched(fetched.pc, fetched.word, state)),
        }
    }
}

impl Latch<Uop> {
    /// Hueco de `stage` con lo que la instrucción lleva al salir de ella.
    pub(crate) fn slot(
        &self,
        stage: Stage,
        state: SlotState,
        bypasses: [Option<Bypass>; 2],
    ) -> Option<Slot> {
        match self {
            Latch::Bubble => None,
            Latch::Fault(halt) => Some(Slot::fault(halt.pc, halt.trap, state)),
            Latch::Holds(uop) => Some(Slot::decoded(
                stage,
                state,
                Progress {
                    pc: uop.pc,
                    word: uop.word,
                    instruction: uop.instruction,
                    operands: [uop.rs1, uop.rs2],
                    next_pc: uop.next_pc,
                    result: uop.result,
                    address: uop.address,
                    data_access: uop.data_access,
                },
                bypasses,
            )),
        }
    }

    /// Lo que impide buscar instrucciones nuevas: una excepción o una `ecall` en vuelo.
    pub(crate) fn blocks_fetch(&self) -> bool {
        match self {
            Latch::Bubble => false,
            Latch::Fault(_) => true,
            Latch::Holds(uop) => uop.operation == Operation::Ecall,
        }
    }

    /// Registro que escribirá en WB lo que lleva este registro de segmentación, si escribe
    /// alguno distinto de `x0`. La `ecall` no cuenta: nada se busca detrás de ella hasta que
    /// se retira, así que nunca hay nadie esperando su resultado en ID.
    pub(crate) fn destination(&self) -> Option<Reg> {
        let Latch::Holds(uop) = self else {
            return None;
        };
        let writes = matches!(
            uop.operation,
            Operation::Alu(..)
                | Operation::Lui
                | Operation::Auipc
                | Operation::Jal
                | Operation::Jalr
                | Operation::Load(_)
        );
        (writes && uop.instruction.rd != Reg::ZERO).then_some(uop.instruction.rd)
    }

    pub(crate) fn holds_load(&self) -> bool {
        matches!(self, Latch::Holds(uop) if matches!(uop.operation, Operation::Load(_)))
    }

    /// Valor que puede adelantar a EX si escribe `reg`: `Some(None)` si lo escribe pero aún no
    /// lo tiene (una carga antes de MEM), `None` si no lo escribe.
    pub(crate) fn produces(&self, reg: Reg) -> Option<Option<u32>> {
        if self.destination() != Some(reg) {
            return None;
        }
        match self {
            Latch::Holds(uop) => Some(uop.result),
            Latch::Bubble | Latch::Fault(_) => None,
        }
    }
}

/// Resultado de la etapa WB cuando no falla.
pub(crate) enum WriteBack {
    Retired { retired: Retired, exit: Option<i32> },
    AwaitingInput,
}

/// IF: busca la palabra de `pc`. Un fallo de búsqueda puede ser de un camino que un salto
/// anterior aún va a vaciar, así que no para nada hasta llegar a WB.
pub(crate) fn fetch(memory: &Memory, pc: u32) -> Latch<Fetched> {
    match memory.fetch(pc) {
        Ok(word) => Latch::Holds(Fetched { pc, word }),
        Err(error) => Latch::Fault(Halt {
            pc,
            trap: Trap::Memory(error),
        }),
    }
}

/// ID: decodifica y lee los registros. Como WB ya ha escrito en este ciclo, se ve lo que
/// retiran las instrucciones de tres etapas por delante, pero no lo de las dos siguientes.
pub(crate) fn decode(fetched: Latch<Fetched>, registers: &Registers) -> Latch<Uop> {
    let fetched = match fetched {
        Latch::Holds(fetched) => fetched,
        Latch::Bubble => return Latch::Bubble,
        Latch::Fault(halt) => return Latch::Fault(halt),
    };
    let fault = |trap| {
        Latch::Fault(Halt {
            pc: fetched.pc,
            trap,
        })
    };
    let instruction = match Instruction::decode(fetched.word) {
        Ok(instruction) => instruction,
        Err(error) => return fault(Trap::Decode(error)),
    };
    let operation = Operation::of(instruction.mnemonic);
    if operation == Operation::Ebreak {
        return fault(Trap::Breakpoint);
    }
    Latch::Holds(Uop {
        pc: fetched.pc,
        word: fetched.word,
        instruction,
        operation,
        rs1: registers.read(instruction.rs1),
        rs2: registers.read(instruction.rs2),
        next_pc: fetched.pc.wrapping_add(4),
        result: None,
        address: None,
        data_access: None,
    })
}

/// MEM: carga o almacenamiento con la dirección calculada en EX.
pub(crate) fn memory_stage(memory: &mut Memory, latch: Latch<Uop>) -> Latch<Uop> {
    let Latch::Holds(mut uop) = latch else {
        return latch;
    };
    let Some(address) = uop.address else {
        return Latch::Holds(uop);
    };
    match access_memory(memory, uop.operation, address, uop.rs2) {
        Ok((loaded, access)) => {
            uop.result = uop.result.or(loaded);
            uop.data_access = access;
            Latch::Holds(uop)
        }
        Err(trap) => Latch::Fault(Halt { pc: uop.pc, trap }),
    }
}

/// WB: atiende la `ecall` si lo es, escribe `rd` y retira. Es el único punto donde cambian
/// los registros y el `pc` arquitectónico.
pub(crate) fn write_back(state: &mut ArchState, uop: &Uop) -> Result<WriteBack, Trap> {
    let mut ecall_write = None;
    let mut exit = None;
    if uop.operation == Operation::Ecall {
        match ecall(state)? {
            EcallEffect::Continue { register_write } => ecall_write = register_write,
            EcallEffect::Exit { code } => exit = Some(code),
            EcallEffect::AwaitingInput => return Ok(WriteBack::AwaitingInput),
        }
    }
    let register_write = uop
        .result
        .filter(|_| uop.instruction.rd != Reg::ZERO)
        .map(|value| RegisterWrite {
            reg: uop.instruction.rd,
            value,
        });
    if let Some(write) = register_write {
        state.registers.write(write.reg, write.value);
    }
    state.pc = uop.next_pc;
    Ok(WriteBack::Retired {
        retired: Retired {
            pc: uop.pc,
            word: uop.word,
            instruction: uop.instruction,
            next_pc: uop.next_pc,
            register_write: register_write.or(ecall_write),
            data_access: uop.data_access,
        },
        exit,
    })
}
