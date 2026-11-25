//! Memoria, modelos de procesador y llamadas al sistema de rvscope.

mod map;
mod memory;
mod processor;
mod state;

pub use map::{
    DATA_SIZE, INITIAL_SP, IO_BASE, IO_END, MapError, MemoryMap, Permissions, Region, STACK_BASE,
    STACK_END, SegmentKind,
};
pub use memory::{AccessKind, Memory, MemoryError, Width};
pub use processor::{DataAccess, Halt, Processor, RegisterWrite, Retired, RunOutcome, Stats, Trap};
pub use state::{ArchState, Registers};
