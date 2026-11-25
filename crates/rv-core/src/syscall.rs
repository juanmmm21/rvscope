//! Llamadas al sistema (`ecall`): consola determinista y los servicios por número en `a7`, con
//! la numeración de RARS que se usa en clase.

use std::collections::VecDeque;

use rv_isa::Reg;
use serde::Serialize;

use crate::memory::{AccessKind, Width};
use crate::processor::{RegisterWrite, Trap};
use crate::state::ArchState;

/// Consola del programa: lo que ha escrito y la entrada que aún no ha leído. Todo en bytes
/// para que imprimir una cadena UTF-8 carácter a carácter dé el mismo texto que de golpe.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Console {
    output: Vec<u8>,
    input: VecDeque<u8>,
    input_closed: bool,
}

impl Console {
    pub fn new() -> Console {
        Console::default()
    }

    pub fn output(&self) -> &[u8] {
        &self.output
    }

    /// Salida como texto; los bytes que no son UTF-8 válido se muestran como �.
    pub fn output_text(&self) -> String {
        String::from_utf8_lossy(&self.output).into_owned()
    }

    /// Añade entrada al final de la cola (lo que el usuario teclea en la interfaz o el stdin de
    /// la CLI).
    pub fn push_input(&mut self, text: &str) {
        self.input.extend(text.as_bytes());
    }

    /// Ya no llegará más entrada: una línea final sin salto vale como línea y leer con la cola
    /// vacía es un error en vez de una espera.
    pub fn close_input(&mut self) {
        self.input_closed = true;
    }

    pub fn is_input_closed(&self) -> bool {
        self.input_closed
    }

    pub fn pending_input(&self) -> usize {
        self.input.len()
    }

    /// Longitud de la siguiente línea, salto incluido, sin consumirla.
    fn next_line_len(&self) -> Read<usize> {
        match self.input.iter().position(|&byte| byte == b'\n') {
            Some(index) => Read::Ready(index + 1),
            None if !self.input_closed => Read::Waiting,
            None if self.input.is_empty() => Read::EndOfInput,
            None => Read::Ready(self.input.len()),
        }
    }

    fn peek(&self, len: usize) -> Vec<u8> {
        self.input.iter().take(len).copied().collect()
    }

    fn consume(&mut self, len: usize) {
        self.input.drain(..len.min(self.input.len()));
    }
}

/// Resultado de mirar la entrada sin tocarla.
enum Read<T> {
    Ready(T),
    Waiting,
    EndOfInput,
}

/// Qué ha pasado al atender una `ecall` que no ha fallado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EcallEffect {
    /// Atendida: la `ecall` se retira y el programa sigue.
    Continue {
        register_write: Option<RegisterWrite>,
    },
    /// Atendida y el programa termina con ese código.
    Exit { code: i32 },
    /// Hace falta entrada que aún no ha llegado: no se toca nada y la `ecall` se repite cuando
    /// la haya.
    AwaitingInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyscallKind {
    Output,
    Input,
    Exit,
}

/// Ficha de un servicio para la chuleta de la interfaz.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SyscallInfo {
    pub number: u32,
    pub name: &'static str,
    pub kind: SyscallKind,
    pub arguments: &'static str,
    pub result: &'static str,
    pub description: &'static str,
}

/// Servicios admitidos. La chuleta sale de aquí y un test comprueba que cada uno se atiende.
pub const SYSCALLS: &[SyscallInfo] = &[
    SyscallInfo {
        number: 1,
        name: "imprimir entero",
        kind: SyscallKind::Output,
        arguments: "a0 = entero",
        result: "",
        description: "Escribe a0 en decimal con signo.",
    },
    SyscallInfo {
        number: 4,
        name: "imprimir cadena",
        kind: SyscallKind::Output,
        arguments: "a0 = dirección",
        result: "",
        description: "Escribe la cadena terminada en cero que empieza en a0.",
    },
    SyscallInfo {
        number: 5,
        name: "leer entero",
        kind: SyscallKind::Input,
        arguments: "",
        result: "a0 = entero",
        description: "Lee una línea con un entero en decimal (admite signo).",
    },
    SyscallInfo {
        number: 8,
        name: "leer cadena",
        kind: SyscallKind::Input,
        arguments: "a0 = búfer, a1 = tamaño",
        result: "",
        description: "Lee una línea y guarda como mucho a1 - 1 bytes (salto incluido) y un cero final.",
    },
    SyscallInfo {
        number: 10,
        name: "salir",
        kind: SyscallKind::Exit,
        arguments: "",
        result: "",
        description: "Termina el programa con código 0.",
    },
    SyscallInfo {
        number: 11,
        name: "imprimir carácter",
        kind: SyscallKind::Output,
        arguments: "a0 = carácter",
        result: "",
        description: "Escribe el byte bajo de a0.",
    },
    SyscallInfo {
        number: 12,
        name: "leer carácter",
        kind: SyscallKind::Input,
        arguments: "",
        result: "a0 = carácter",
        description: "Lee un byte de la entrada.",
    },
    SyscallInfo {
        number: 17,
        name: "salir con código",
        kind: SyscallKind::Exit,
        arguments: "a0 = código",
        result: "",
        description: "Termina el programa con el código de a0.",
    },
    SyscallInfo {
        number: 34,
        name: "imprimir hexadecimal",
        kind: SyscallKind::Output,
        arguments: "a0 = entero",
        result: "",
        description: "Escribe a0 en hexadecimal con 8 cifras (0x0000002a).",
    },
    SyscallInfo {
        number: 35,
        name: "imprimir binario",
        kind: SyscallKind::Output,
        arguments: "a0 = entero",
        result: "",
        description: "Escribe los 32 bits de a0.",
    },
    SyscallInfo {
        number: 36,
        name: "imprimir sin signo",
        kind: SyscallKind::Output,
        arguments: "a0 = entero",
        result: "",
        description: "Escribe a0 en decimal sin signo.",
    },
    SyscallInfo {
        number: 93,
        name: "salir con código",
        kind: SyscallKind::Exit,
        arguments: "a0 = código",
        result: "",
        description: "Igual que el 17; es el número de exit en Linux.",
    },
];

/// Atiende la `ecall` con el estado actual. Si falla o espera entrada, el estado queda intacto;
/// el `pc` lo avanza quien llama.
pub fn ecall(state: &mut ArchState) -> Result<EcallEffect, Trap> {
    let number = state.registers.read(Reg::A7);
    let a0 = state.registers.read(Reg::A0);
    let a1 = state.registers.read(Reg::A1);
    let print = |state: &mut ArchState, text: String| {
        state.console.output.extend(text.as_bytes());
        EcallEffect::Continue {
            register_write: None,
        }
    };
    match number {
        1 => Ok(print(state, (a0 as i32).to_string())),
        4 => {
            let text = read_c_string(state, a0)?;
            state.console.output.extend(text);
            Ok(EcallEffect::Continue {
                register_write: None,
            })
        }
        11 => {
            state.console.output.push(a0 as u8);
            Ok(EcallEffect::Continue {
                register_write: None,
            })
        }
        34 => Ok(print(state, format!("0x{a0:08x}"))),
        35 => Ok(print(state, format!("{a0:032b}"))),
        36 => Ok(print(state, a0.to_string())),
        5 => read_int(state),
        8 => read_string(state, a0, a1 as i32),
        12 => read_char(state),
        10 => Ok(EcallEffect::Exit { code: 0 }),
        17 | 93 => Ok(EcallEffect::Exit { code: a0 as i32 }),
        _ => Err(Trap::UnknownSyscall { number }),
    }
}

/// Bytes hasta el cero final, sin incluirlo. No hace falta tope: la memoria sin escribir vale
/// cero y salirse de un segmento es un error.
fn read_c_string(state: &ArchState, address: u32) -> Result<Vec<u8>, Trap> {
    let mut bytes = Vec::new();
    let mut current = address;
    loop {
        let byte = state.memory.load(current, Width::Byte)? as u8;
        if byte == 0 {
            return Ok(bytes);
        }
        bytes.push(byte);
        current = current.wrapping_add(1);
    }
}

fn write_to(state: &mut ArchState, reg: Reg, value: u32) -> EcallEffect {
    state.registers.write(reg, value);
    EcallEffect::Continue {
        register_write: (reg != Reg::ZERO).then_some(RegisterWrite { reg, value }),
    }
}

fn read_int(state: &mut ArchState) -> Result<EcallEffect, Trap> {
    let len = match state.console.next_line_len() {
        Read::Ready(len) => len,
        Read::Waiting => return Ok(EcallEffect::AwaitingInput),
        Read::EndOfInput => return Err(Trap::EndOfInput),
    };
    let line = state.console.peek(len);
    let value = std::str::from_utf8(&line)
        .ok()
        .and_then(|text| text.trim().parse::<i32>().ok())
        .ok_or(Trap::InvalidInteger)?;
    state.console.consume(len);
    Ok(write_to(state, Reg::A0, value as u32))
}

fn read_char(state: &mut ArchState) -> Result<EcallEffect, Trap> {
    let Some(&byte) = state.console.input.front() else {
        return if state.console.input_closed {
            Err(Trap::EndOfInput)
        } else {
            Ok(EcallEffect::AwaitingInput)
        };
    };
    state.console.consume(1);
    Ok(write_to(state, Reg::A0, u32::from(byte)))
}

/// Como en RARS, la línea se consume entera aunque no quepa en el búfer.
fn read_string(state: &mut ArchState, buffer: u32, size: i32) -> Result<EcallEffect, Trap> {
    let len = match state.console.next_line_len() {
        Read::Ready(len) => len,
        Read::Waiting => return Ok(EcallEffect::AwaitingInput),
        Read::EndOfInput => return Err(Trap::EndOfInput),
    };
    let capacity = usize::try_from(size).unwrap_or(0);
    let mut bytes = state.console.peek(len.min(capacity.saturating_sub(1)));
    if capacity > 0 {
        bytes.push(0);
    }
    // Se comprueba todo el búfer antes de escribir para que un fallo a medias no deje la
    // memoria cambiada.
    for offset in 0..bytes.len() {
        let address = buffer.wrapping_add(offset as u32);
        state
            .memory
            .check(address, Width::Byte, AccessKind::Store)?;
    }
    state.memory.poke(buffer, &bytes);
    state.console.consume(len);
    Ok(EcallEffect::Continue {
        register_write: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryError;

    fn state(source: &str) -> ArchState {
        ArchState::for_program(&rv_asm::assemble(source).unwrap()).unwrap()
    }

    fn call(state: &mut ArchState, number: u32, a0: u32) -> Result<EcallEffect, Trap> {
        state.registers.write(Reg::A7, number);
        state.registers.write(Reg::A0, a0);
        ecall(state)
    }

    const CONTINUE: EcallEffect = EcallEffect::Continue {
        register_write: None,
    };

    #[test]
    fn prints_numbers_in_every_base() {
        let mut state = state("nop\n");
        for number in [1, 36, 34, 35] {
            assert_eq!(call(&mut state, number, -42i32 as u32), Ok(CONTINUE));
            state.console.output.push(b'|');
        }
        assert_eq!(
            state.console.output_text(),
            "-42|4294967254|0xffffffd6|11111111111111111111111111010110|"
        );
    }

    #[test]
    fn prints_strings_and_characters_as_bytes() {
        let mut state = state(".data\nmsg: .string \"año\\n\"\n.text\nnop\n");
        call(&mut state, 4, 0x1000_0000).unwrap();
        // «ñ» llega como sus dos bytes UTF-8 por separado y se lee igual.
        for byte in "ñ!".bytes() {
            call(&mut state, 11, u32::from(byte) | 0xffff_ff00).unwrap();
        }
        assert_eq!(state.console.output_text(), "año\nñ!");
    }

    #[test]
    fn printing_a_bad_string_address_traps_without_output() {
        let mut state = state("nop\n");
        assert_eq!(
            call(&mut state, 4, 0x2000_0000),
            Err(Trap::Memory(MemoryError::Unmapped {
                address: 0x2000_0000,
                access: AccessKind::Load
            }))
        );
        assert!(state.console.output().is_empty());
    }

    #[test]
    fn reading_integers_waits_for_a_whole_line() {
        let mut state = state("nop\n");
        assert_eq!(call(&mut state, 5, 0), Ok(EcallEffect::AwaitingInput));
        state.console.push_input("  -17");
        let before = state.clone();
        assert_eq!(call(&mut state, 5, 0), Ok(EcallEffect::AwaitingInput));
        assert_eq!(state, before);
        state.console.push_input("\n2147483647\n");
        assert_eq!(
            call(&mut state, 5, 0),
            Ok(EcallEffect::Continue {
                register_write: Some(RegisterWrite {
                    reg: Reg::A0,
                    value: -17i32 as u32
                })
            })
        );
        call(&mut state, 5, 0).unwrap();
        assert_eq!(state.registers.read(Reg::A0), 0x7fff_ffff);
        assert_eq!(state.console.pending_input(), 0);
    }

    #[test]
    fn invalid_integers_and_end_of_input_trap() {
        let mut state = state("nop\n");
        state.console.push_input("doce\n");
        assert_eq!(call(&mut state, 5, 0), Err(Trap::InvalidInteger));
        // La línea mala no se consume: el estado sigue como antes de la ecall.
        assert_eq!(state.console.pending_input(), 5);

        let mut state = self::state("nop\n");
        state.console.push_input("5");
        state.console.close_input();
        call(&mut state, 5, 0).unwrap();
        assert_eq!(state.registers.read(Reg::A0), 5);
        assert_eq!(call(&mut state, 5, 0), Err(Trap::EndOfInput));
        assert_eq!(call(&mut state, 12, 0), Err(Trap::EndOfInput));
    }

    #[test]
    fn reads_characters_one_byte_at_a_time() {
        let mut state = state("nop\n");
        assert_eq!(call(&mut state, 12, 0), Ok(EcallEffect::AwaitingInput));
        state.console.push_input("ok");
        call(&mut state, 12, 0).unwrap();
        assert_eq!(state.registers.read(Reg::A0), u32::from(b'o'));
        call(&mut state, 12, 0).unwrap();
        assert_eq!(state.registers.read(Reg::A0), u32::from(b'k'));
    }

    #[test]
    fn reads_strings_into_memory_truncated_to_the_buffer() {
        let mut state = state(".data\nbuf: .space 8\n.text\nnop\n");
        state.console.push_input("hola\nrvscope largo\n");
        state.registers.write(Reg::A1, 8);
        call(&mut state, 8, 0x1000_0000).unwrap();
        assert_eq!(state.memory.peek(0x1000_0000, 8), b"hola\n\0\0\0");
        call(&mut state, 8, 0x1000_0000).unwrap();
        assert_eq!(state.memory.peek(0x1000_0000, 8), b"rvscope\0");
        assert_eq!(state.console.pending_input(), 0);
    }

    #[test]
    fn reading_a_string_into_code_changes_nothing() {
        let mut state = state("nop\nnop\n");
        state.console.push_input("abcdef\n");
        state.registers.write(Reg::A7, 8);
        state.registers.write(Reg::A1, 16);
        let before = state.clone();
        let result = ecall(&mut state);
        assert!(matches!(
            result,
            Err(Trap::Memory(MemoryError::PermissionDenied { .. }))
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn exits_and_unknown_services() {
        let mut state = state("nop\n");
        assert_eq!(call(&mut state, 10, 3), Ok(EcallEffect::Exit { code: 0 }));
        assert_eq!(
            call(&mut state, 93, -1i32 as u32),
            Ok(EcallEffect::Exit { code: -1 })
        );
        assert_eq!(call(&mut state, 17, 2), Ok(EcallEffect::Exit { code: 2 }));
        assert_eq!(
            call(&mut state, 999, 0),
            Err(Trap::UnknownSyscall { number: 999 })
        );
    }

    #[test]
    fn every_listed_service_is_handled() {
        let numbers: Vec<u32> = SYSCALLS.iter().map(|info| info.number).collect();
        assert!(
            numbers.windows(2).all(|pair| pair[0] < pair[1]),
            "la tabla va ordenada y sin repetidos"
        );
        for info in SYSCALLS {
            let mut state = state(".data\ns: .string \"\"\n.text\nnop\n");
            state.console.push_input("1\n");
            state.registers.write(Reg::A1, 4);
            let result = call(&mut state, info.number, 0x1000_0000);
            assert!(
                !matches!(result, Err(Trap::UnknownSyscall { .. })),
                "{}",
                info.name
            );
        }
    }
}
