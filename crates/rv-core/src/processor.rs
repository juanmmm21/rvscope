//! Contrato común de los modelos de procesador y lo que cuentan de cada ciclo.

use rv_isa::{DecodeError, Instruction, Reg};
use serde::Serialize;
use thiserror::Error;

use crate::memory::{AccessKind, MemoryError, Width};
use crate::state::ArchState;
use crate::syscall::Console;

/// Motivo por el que el procesador se detiene. La instrucción que lo provoca no se retira:
/// el estado queda como justo antes de ejecutarla.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Trap {
    #[error(transparent)]
    Memory(#[from] MemoryError),
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error("salto a 0x{target:08x}: las instrucciones deben estar alineadas a 4 bytes")]
    MisalignedTarget { target: u32 },
    #[error("ecall: servicio {number} desconocido (mira el número que hay en a7)")]
    UnknownSyscall { number: u32 },
    #[error("ecall: lo leído no es un entero válido")]
    InvalidInteger,
    #[error("ecall: no queda entrada que leer")]
    EndOfInput,
    /// Fin normal del programa con una `ecall` de salida, que sí se retira.
    #[error("el programa terminó con código {code}")]
    Exited { code: i32 },
    #[error("punto de parada (ebreak)")]
    Breakpoint,
}

/// Parada definitiva: dónde y por qué.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq, Serialize)]
#[error("0x{pc:08x}: {trap}")]
pub struct Halt {
    pub pc: u32,
    pub trap: Trap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct RegisterWrite {
    pub reg: Reg,
    pub value: u32,
}

/// Acceso a datos de una carga o un almacenamiento; en las cargas, `value` es lo leído ya
/// extendido y en los almacenamientos, lo escrito (solo cuentan los bytes de `width`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct DataAccess {
    pub kind: AccessKind,
    pub address: u32,
    pub width: Width,
    pub value: u32,
}

/// Instrucción que termina y deja su efecto en el estado arquitectónico. La interfaz la usa
/// para resaltar lo que acaba de cambiar y la CLI para la traza.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Retired {
    pub pc: u32,
    pub word: u32,
    pub instruction: Instruction,
    pub next_pc: u32,
    /// `None` si la instrucción no escribe registro o escribe en `x0`.
    pub register_write: Option<RegisterWrite>,
    pub data_access: Option<DataAccess>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Stats {
    pub cycles: u64,
    pub retired: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    Halted(Halt),
    /// Se agotaron los ciclos pedidos sin pararse: un bucle infinito no cuelga la interfaz.
    CycleLimit,
    /// Una `ecall` de lectura espera entrada; se sigue tras añadirla a la consola.
    AwaitingInput,
}

/// Modelo de procesador. Todos comparten el mismo estado arquitectónico final para un mismo
/// programa; cambian la temporización y lo que se ve por dentro.
pub trait Processor {
    fn name(&self) -> &'static str;

    fn state(&self) -> &ArchState;

    fn stats(&self) -> Stats;

    fn halt(&self) -> Option<Halt>;

    /// Consola del programa, para darle entrada (la salida se lee en `state().console`).
    fn console_mut(&mut self) -> &mut Console;

    /// Cierto si el último ciclo no avanzó porque una `ecall` espera entrada. Esos ciclos no
    /// cuentan: la traza es la misma tarde lo que tarde en llegar la entrada.
    fn awaiting_input(&self) -> bool;

    /// Avanza un ciclo de reloj y devuelve las instrucciones retiradas en él (ninguna, una o
    /// varias según el modelo). Una vez parado no hace nada.
    fn step(&mut self) -> Vec<Retired>;

    /// Ejecuta hasta pararse o hasta `max_cycles` ciclos.
    fn run(&mut self, max_cycles: u64) -> RunOutcome {
        for _ in 0..max_cycles {
            if let Some(halt) = self.halt() {
                return RunOutcome::Halted(halt);
            }
            self.step();
            if self.awaiting_input() {
                return RunOutcome::AwaitingInput;
            }
        }
        match self.halt() {
            Some(halt) => RunOutcome::Halted(halt),
            None => RunOutcome::CycleLimit,
        }
    }
}
