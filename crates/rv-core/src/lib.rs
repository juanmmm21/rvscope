//! Memoria, modelos de procesador y llamadas al sistema de rvscope.

mod exec;
mod five_stage;
mod map;
mod memory;
mod pipeline;
mod processor;
mod single_cycle;
mod state;
mod syscall;

pub use exec::{AluOp, AluSource, BranchCond, LoadOp, Operation};
pub use five_stage::{FiveStage, Hazards};
pub use map::{
    DATA_SIZE, INITIAL_SP, IO_BASE, IO_END, MapError, MemoryMap, Permissions, Region, STACK_BASE,
    STACK_END, SegmentKind,
};
pub use memory::{AccessKind, Memory, MemoryError, Width};
pub use pipeline::Bypass;
pub use processor::{DataAccess, Halt, Processor, RegisterWrite, Retired, RunOutcome, Stats, Trap};
pub use single_cycle::SingleCycle;
pub use state::{ArchState, Registers};
pub use syscall::{Console, EcallEffect, SYSCALLS, SyscallInfo, SyscallKind, ecall};
