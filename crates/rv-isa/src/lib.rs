//! Tabla de instrucciones de RISC-V (RV32I + M), codificación y decodificación.
//!
//! La tabla ([`INSTRUCTIONS`] y [`PSEUDO_INSTRUCTIONS`]) es la fuente única de lo que admite
//! rvscope: el ensamblador, el simulador y la chuleta de la interfaz salen de ella.

mod format;
mod instruction;
mod layout;
mod pseudo;
mod register;
mod table;

pub use format::{BitField, Format};
pub use instruction::{DecodeError, EncodeError, Instruction, fence_set};
pub use layout::EncodedField;
pub use pseudo::{PSEUDO_INSTRUCTIONS, PseudoSpec};
pub use register::{REGISTERS, Reg, RegisterInfo, RegisterStyle, Saver};
pub use table::{
    Encoding, Extension, Group, INSTRUCTIONS, InstructionSpec, Mnemonic, Operands, find,
};
