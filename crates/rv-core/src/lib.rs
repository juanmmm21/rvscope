//! Memoria, modelos de procesador y llamadas al sistema de rvscope.

mod map;

pub use map::{
    DATA_SIZE, INITIAL_SP, IO_BASE, IO_END, MapError, MemoryMap, Permissions, Region, STACK_BASE,
    STACK_END, SegmentKind,
};
