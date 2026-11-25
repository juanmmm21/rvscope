//! Banco de registros enteros: índices, nombres y convenio de llamada del ABI.

use serde::Serialize;

/// Registro entero `x0`..`x31`. El constructor garantiza que el índice es válido,
/// así que indexar con él nunca se sale del banco.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Reg(u8);

/// Cómo se escriben los registros al desensamblar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RegisterStyle {
    /// Nombres del ABI: `zero`, `ra`, `sp`, `a0`...
    #[default]
    Abi,
    /// Nombres numéricos: `x0`, `x1`...
    Numeric,
}

/// Quién conserva el valor del registro a través de una llamada a función.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Saver {
    /// No aplica: constante cero o registro reservado del sistema.
    None,
    /// Lo guarda quien llama si lo necesita después de la llamada.
    Caller,
    /// Lo guarda la función llamada si lo modifica.
    Callee,
}

/// Ficha de un registro para la chuleta.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct RegisterInfo {
    pub index: u8,
    pub name: &'static str,
    pub abi: &'static str,
    pub role: &'static str,
    pub saver: Saver,
}

const NUMERIC_NAMES: [&str; 32] = [
    "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11", "x12", "x13", "x14",
    "x15", "x16", "x17", "x18", "x19", "x20", "x21", "x22", "x23", "x24", "x25", "x26", "x27",
    "x28", "x29", "x30", "x31",
];

const ABI_NAMES: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
    "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
    "t5", "t6",
];

impl Reg {
    pub const ZERO: Reg = Reg(0);
    pub const RA: Reg = Reg(1);
    pub const SP: Reg = Reg(2);
    pub const GP: Reg = Reg(3);
    pub const T0: Reg = Reg(5);
    pub const T1: Reg = Reg(6);
    pub const A0: Reg = Reg(10);
    pub const A1: Reg = Reg(11);
    pub const A7: Reg = Reg(17);

    /// Registro con ese índice, si está entre 0 y 31.
    pub const fn new(index: u8) -> Option<Reg> {
        if index < 32 { Some(Reg(index)) } else { None }
    }

    /// Registro a partir de un campo de 5 bits de una instrucción (los bits sobrantes se ignoran).
    pub const fn from_field(bits: u32) -> Reg {
        Reg((bits & 0x1f) as u8)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }

    pub const fn bits(self) -> u32 {
        self.0 as u32
    }

    pub fn abi_name(self) -> &'static str {
        ABI_NAMES[self.index()]
    }

    pub fn numeric_name(self) -> &'static str {
        NUMERIC_NAMES[self.index()]
    }

    pub fn name(self, style: RegisterStyle) -> &'static str {
        match style {
            RegisterStyle::Abi => self.abi_name(),
            RegisterStyle::Numeric => self.numeric_name(),
        }
    }

    /// Acepta `x0`..`x31`, los nombres del ABI y `fp` (alias de `s0`), sin distinguir mayúsculas.
    pub fn parse(text: &str) -> Option<Reg> {
        let lower = text.to_ascii_lowercase();
        if lower == "fp" {
            return Some(Reg(8));
        }
        if let Some(position) = ABI_NAMES.iter().position(|name| *name == lower) {
            return Reg::new(u8::try_from(position).ok()?);
        }
        let digits = lower.strip_prefix('x')?;
        // Rechaza ceros a la izquierda («x01») para que cada registro tenga un único nombre numérico.
        if digits.is_empty() || (digits.len() > 1 && digits.starts_with('0')) {
            return None;
        }
        digits.parse::<u8>().ok().and_then(Reg::new)
    }
}

/// Fichas de los 32 registros enteros, en orden.
pub static REGISTERS: [RegisterInfo; 32] = [
    info(0, "Constante cero", Saver::None),
    info(1, "Dirección de retorno", Saver::Caller),
    info(2, "Puntero de pila", Saver::Callee),
    info(3, "Puntero global", Saver::None),
    info(4, "Puntero de hilo", Saver::None),
    info(5, "Temporal / enlace alternativo", Saver::Caller),
    info(6, "Temporal", Saver::Caller),
    info(7, "Temporal", Saver::Caller),
    info(8, "Guardado / puntero de marco (fp)", Saver::Callee),
    info(9, "Guardado", Saver::Callee),
    info(10, "Argumento / valor de retorno", Saver::Caller),
    info(11, "Argumento / valor de retorno", Saver::Caller),
    info(12, "Argumento", Saver::Caller),
    info(13, "Argumento", Saver::Caller),
    info(14, "Argumento", Saver::Caller),
    info(15, "Argumento", Saver::Caller),
    info(16, "Argumento", Saver::Caller),
    info(17, "Argumento / número de ecall", Saver::Caller),
    info(18, "Guardado", Saver::Callee),
    info(19, "Guardado", Saver::Callee),
    info(20, "Guardado", Saver::Callee),
    info(21, "Guardado", Saver::Callee),
    info(22, "Guardado", Saver::Callee),
    info(23, "Guardado", Saver::Callee),
    info(24, "Guardado", Saver::Callee),
    info(25, "Guardado", Saver::Callee),
    info(26, "Guardado", Saver::Callee),
    info(27, "Guardado", Saver::Callee),
    info(28, "Temporal", Saver::Caller),
    info(29, "Temporal", Saver::Caller),
    info(30, "Temporal", Saver::Caller),
    info(31, "Temporal", Saver::Caller),
];

const fn info(index: u8, role: &'static str, saver: Saver) -> RegisterInfo {
    RegisterInfo {
        index,
        name: NUMERIC_NAMES[index as usize],
        abi: ABI_NAMES[index as usize],
        role,
        saver,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numeric_abi_and_alias_names() {
        assert_eq!(Reg::parse("x0"), Some(Reg::ZERO));
        assert_eq!(Reg::parse("X31").map(Reg::index), Some(31));
        assert_eq!(Reg::parse("sp"), Some(Reg::SP));
        assert_eq!(Reg::parse("fp").map(Reg::index), Some(8));
        assert_eq!(Reg::parse("S11").map(Reg::index), Some(27));
        assert_eq!(Reg::parse("t6").map(Reg::index), Some(31));
    }

    #[test]
    fn rejects_invalid_names() {
        for name in ["x32", "x", "x01", "a8", "s12", "", "zero0", "r1"] {
            assert_eq!(Reg::parse(name), None, "{name}");
        }
    }

    #[test]
    fn register_table_matches_names() {
        for (position, register) in REGISTERS.iter().enumerate() {
            assert_eq!(usize::from(register.index), position);
            let reg = Reg::new(register.index);
            assert_eq!(reg.map(Reg::abi_name), Some(register.abi));
            assert_eq!(Reg::parse(register.abi), reg);
            assert_eq!(Reg::parse(register.name), reg);
        }
    }
}
