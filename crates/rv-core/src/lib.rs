//! Memoria, modelos de procesador y llamadas al sistema de rvscope.

mod dual_issue;
mod exec;
mod five_stage;
mod map;
mod memory;
mod model;
mod pipeline;
mod processor;
mod single_cycle;
mod state;
mod syscall;
mod view;
mod wires;

pub use dual_issue::DualIssue;
pub use exec::{AluOp, AluSource, BranchCond, LoadOp, Operation};
pub use five_stage::{FiveStage, Hazards};
pub use map::{
    DATA_SIZE, INITIAL_SP, IO_BASE, IO_END, MapError, MemoryMap, Permissions, Region, STACK_BASE,
    STACK_END, SegmentKind,
};
pub use memory::{AccessKind, Memory, MemoryError, Width};
pub use model::{Machine, Model};
pub use pipeline::Bypass;
pub use processor::{DataAccess, Halt, Processor, RegisterWrite, Retired, RunOutcome, Stats, Trap};
pub use single_cycle::SingleCycle;
pub use state::{ArchState, Registers};
pub use syscall::{Console, EcallEffect, SYSCALLS, SyscallInfo, SyscallKind, ecall};
pub use view::{Control, Operand, PipelineView, Slot, SlotState, Stage, StageView, WriteSource};
pub use wires::{AluFunction, AluInput, AluWires, PcSource, Wires};
