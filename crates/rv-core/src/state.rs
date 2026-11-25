//! Estado arquitectónico: lo que un programa puede observar (pc, registros y memoria).

use rv_asm::Program;
use rv_isa::Reg;
use serde::Serialize;

use crate::map::MapError;
use crate::memory::Memory;
use crate::syscall::Console;

/// Banco de 32 registros enteros; `x0` vale siempre cero.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Registers {
    values: [u32; 32],
}

impl Registers {
    pub fn new() -> Registers {
        Registers::default()
    }

    pub fn read(&self, reg: Reg) -> u32 {
        self.values.get(reg.index()).copied().unwrap_or(0)
    }

    /// Las escrituras en `x0` se descartan, como en el hardware.
    pub fn write(&mut self, reg: Reg, value: u32) {
        if reg == Reg::ZERO {
            return;
        }
        if let Some(slot) = self.values.get_mut(reg.index()) {
            *slot = value;
        }
    }

    pub fn values(&self) -> &[u32; 32] {
        &self.values
    }
}

/// Estado que todos los modelos de procesador deben dejar igual al terminar un programa. La
/// consola cuenta: lo que un programa imprime también es observable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchState {
    pub pc: u32,
    pub registers: Registers,
    pub memory: Memory,
    pub console: Console,
}

impl ArchState {
    /// Estado inicial: el programa cargado, `pc` en la entrada y `sp` en lo alto de la pila.
    pub fn for_program(program: &Program) -> Result<ArchState, MapError> {
        let memory = Memory::with_program(program)?;
        let mut registers = Registers::new();
        registers.write(Reg::SP, memory.map().initial_sp());
        Ok(ArchState {
            pc: program.entry,
            registers,
            memory,
            console: Console::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::INITIAL_SP;

    #[test]
    fn zero_register_ignores_writes() {
        let mut registers = Registers::new();
        registers.write(Reg::ZERO, 7);
        registers.write(Reg::A0, 7);
        assert_eq!(registers.read(Reg::ZERO), 0);
        assert_eq!(registers.read(Reg::A0), 7);
    }

    #[test]
    fn initial_state_follows_the_program() {
        let program = rv_asm::assemble("nop\nmain: nop\n.globl main\n").unwrap();
        let state = ArchState::for_program(&program).unwrap();
        assert_eq!(state.pc, program.entry);
        assert_eq!(state.registers.read(Reg::SP), INITIAL_SP);
        assert_eq!(state.registers.read(Reg::RA), 0);
    }
}
