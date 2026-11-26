//! Evaluación de expresiones con la tabla de símbolos.

use std::collections::HashMap;

use crate::error::{LineError, LineResult, Span};
use crate::parser::{BinaryOp, Expr, LocalRef, Modifier};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolValue {
    /// Dirección de una etiqueta.
    Label(u32),
    /// Constante de `.equ` / `.set`.
    Constant(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SymbolDef {
    pub value: SymbolValue,
    pub line: usize,
}

/// En la primera pasada aún no se conocen las etiquetas que vienen después.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    Layout,
    Final,
}

pub struct Context<'a> {
    pub symbols: &'a HashMap<String, SymbolDef>,
    pub pass: Pass,
    /// Dirección de la sentencia que se evalúa.
    pub here: u32,
}

/// `%hi`: parte alta que, sumada a `%lo` con signo, reconstruye el valor.
pub fn hi20(value: u32) -> u32 {
    (value.wrapping_add(0x800) >> 12) & 0xf_ffff
}

/// `%lo`: los 12 bits bajos con signo.
pub fn lo12(value: u32) -> i32 {
    (((value & 0xfff) << 20) as i32) >> 20
}

/// Clave de una etiqueta local en la tabla de símbolos. Lleva un espacio, que ningún nombre
/// escrito en el fuente puede tener, así que no choca con etiquetas normales.
pub fn local_key(number: u32, ordinal: usize) -> String {
    format!("{number} {ordinal}")
}

pub fn is_local_key(name: &str) -> bool {
    name.contains(' ')
}

/// Trunca a 32 bits, como hace el hardware con cualquier dirección o constante.
pub fn wrap32(value: i64) -> u32 {
    (value & 0xffff_ffff) as u32
}

impl Context<'_> {
    /// Valor de una expresión que puede usar etiquetas y constantes.
    pub fn value(&self, expr: &Expr) -> LineResult<i64> {
        self.eval(expr, &|symbol, span| {
            let name = match symbol {
                Name::Named(name) => name.to_owned(),
                Name::Local(local) => local_key(local.number, local.ordinal),
            };
            match self.symbols.get(&name) {
                Some(SymbolDef {
                    value: SymbolValue::Label(address),
                    ..
                }) => Ok(i64::from(*address)),
                Some(SymbolDef {
                    value: SymbolValue::Constant(value),
                    ..
                }) => Ok(*value),
                // En la primera pasada solo importa el tamaño, que no depende de las etiquetas.
                // Una etiqueta que aún no se conoce vale la dirección actual: así un salto hacia
                // delante no se sale de rango solo porque el código empiece lejos del 0.
                None if self.pass == Pass::Layout => Ok(i64::from(self.here)),
                None => Err(LineError::new(
                    span,
                    match symbol {
                        Name::Named(name) => format!("etiqueta no definida «{name}»"),
                        Name::Local(local) => format!(
                            "«{}»: no hay ninguna etiqueta «{}:» después de esta línea",
                            local.text(),
                            local.number
                        ),
                    },
                )),
            }
        })
    }

    /// Valor de una expresión que solo puede usar constantes ya definidas: de ella depende
    /// el tamaño de lo que se emite, así que debe conocerse en la primera pasada.
    pub fn constant(&self, expr: &Expr) -> LineResult<i64> {
        self.eval(expr, &|symbol, span| {
            let name = match symbol {
                Name::Named(name) => name,
                Name::Local(local) => {
                    return Err(LineError::new(
                        span,
                        format!("«{}» es una etiqueta; aquí hace falta una constante", local.text()),
                    ));
                }
            };
            match self.symbols.get(name) {
            Some(SymbolDef {
                value: SymbolValue::Constant(value),
                ..
            }) => Ok(*value),
            Some(SymbolDef {
                value: SymbolValue::Label(_),
                ..
            }) => Err(LineError::new(span, format!("«{name}» es una etiqueta; aquí hace falta una constante"))),
            None => Err(LineError::new(span, format!("constante no definida «{name}» (las constantes se definen con .equ antes de usarlas)"))),
        }
        })
    }

    fn eval(
        &self,
        expr: &Expr,
        resolve: &dyn Fn(Name<'_>, Span) -> LineResult<i64>,
    ) -> LineResult<i64> {
        let overflow = || LineError::new(Span::default(), "la expresión se desborda");
        match expr {
            Expr::Number(value) => Ok(*value),
            Expr::Symbol(name, span) => resolve(Name::Named(name), *span),
            Expr::Local(local) => resolve(Name::Local(*local), local.span),
            Expr::Negate(inner) => self
                .eval(inner, resolve)?
                .checked_neg()
                .ok_or_else(overflow),
            Expr::Not(inner) => Ok(!self.eval(inner, resolve)?),
            Expr::Binary(op, left, right) => {
                let (left, right) = (self.eval(left, resolve)?, self.eval(right, resolve)?);
                binary(*op, left, right)
            }
            Expr::Modified(modifier, inner) => {
                let value = wrap32(self.eval(inner, resolve)?);
                Ok(match modifier {
                    Modifier::Hi => i64::from(hi20(value)),
                    Modifier::Lo => i64::from(lo12(value)),
                })
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Name<'a> {
    Named(&'a str),
    Local(LocalRef),
}

/// Operación en 64 bits. Suma, resta y producto avisan si se desbordan; los desplazamientos y
/// las operaciones de bits trabajan con complemento a dos, que es lo que se espera al escribir
/// máscaras como `(1 << 32) - 1`.
fn binary(op: BinaryOp, left: i64, right: i64) -> LineResult<i64> {
    let error = |message: &str| LineError::new(Span::default(), message);
    let overflow = || error("la expresión se desborda");
    let shift = || {
        u32::try_from(right)
            .ok()
            .filter(|amount| *amount < 64)
            .ok_or_else(|| error("el desplazamiento debe ir de 0 a 63"))
    };
    match op {
        BinaryOp::Add => left.checked_add(right).ok_or_else(overflow),
        BinaryOp::Sub => left.checked_sub(right).ok_or_else(overflow),
        BinaryOp::Mul => left.checked_mul(right).ok_or_else(overflow),
        BinaryOp::Div if right == 0 => Err(error("división por cero en la expresión")),
        BinaryOp::Div => left.checked_div(right).ok_or_else(overflow),
        BinaryOp::Shl => Ok(left.wrapping_shl(shift()?)),
        BinaryOp::Shr => Ok(left >> shift()?),
        BinaryOp::And => Ok(left & right),
        BinaryOp::Or => Ok(left | right),
        BinaryOp::Xor => Ok(left ^ right),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hi_and_lo_rebuild_the_value() {
        for value in [
            0u32,
            0x7ff,
            0x800,
            0xfff,
            0x1234_5678,
            0xffff_f800,
            0xffff_ffff,
            0x8000_0000,
        ] {
            let rebuilt = (hi20(value) << 12).wrapping_add(lo12(value) as u32);
            assert_eq!(rebuilt, value, "0x{value:08x}");
        }
        assert_eq!(hi20(0x800), 1);
        assert_eq!(lo12(0x800), -2048);
    }

    #[test]
    fn binary_operators() {
        assert_eq!(binary(BinaryOp::Shl, 1, 32), Ok(1 << 32));
        assert_eq!(binary(BinaryOp::Shr, -8, 1), Ok(-4));
        assert_eq!(
            binary(BinaryOp::And, -0x8000_0000, 0xffff_ffff),
            Ok(0x8000_0000)
        );
        assert_eq!(binary(BinaryOp::Or, 0xf0, 0x0f), Ok(0xff));
        assert_eq!(binary(BinaryOp::Xor, 0xff, 0x0f), Ok(0xf0));
        assert_eq!(binary(BinaryOp::Div, -7, 2), Ok(-3));
        assert!(binary(BinaryOp::Div, 1, 0).is_err());
        assert!(binary(BinaryOp::Shl, 1, 64).is_err());
        assert!(binary(BinaryOp::Shr, 1, -1).is_err());
        assert!(binary(BinaryOp::Mul, i64::MAX, 2).is_err());
    }
}
