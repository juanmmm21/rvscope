//! Evaluación de expresiones con la tabla de símbolos.

use std::collections::HashMap;

use crate::error::{LineError, LineResult, Span};
use crate::parser::{Expr, Modifier};

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
}

/// `%hi`: parte alta que, sumada a `%lo` con signo, reconstruye el valor.
pub fn hi20(value: u32) -> u32 {
    (value.wrapping_add(0x800) >> 12) & 0xf_ffff
}

/// `%lo`: los 12 bits bajos con signo.
pub fn lo12(value: u32) -> i32 {
    (((value & 0xfff) << 20) as i32) >> 20
}

/// Trunca a 32 bits, como hace el hardware con cualquier dirección o constante.
pub fn wrap32(value: i64) -> u32 {
    (value & 0xffff_ffff) as u32
}

impl Context<'_> {
    /// Valor de una expresión que puede usar etiquetas y constantes.
    pub fn value(&self, expr: &Expr) -> LineResult<i64> {
        self.eval(expr, &|name, span| match self.symbols.get(name) {
            Some(SymbolDef {
                value: SymbolValue::Label(address),
                ..
            }) => Ok(i64::from(*address)),
            Some(SymbolDef {
                value: SymbolValue::Constant(value),
                ..
            }) => Ok(*value),
            // En la primera pasada solo importa el tamaño, que no depende de las etiquetas.
            None if self.pass == Pass::Layout => Ok(0),
            None => Err(LineError::new(
                span,
                format!("etiqueta no definida «{name}»"),
            )),
        })
    }

    /// Valor de una expresión que solo puede usar constantes ya definidas: de ella depende
    /// el tamaño de lo que se emite, así que debe conocerse en la primera pasada.
    pub fn constant(&self, expr: &Expr) -> LineResult<i64> {
        self.eval(expr, &|name, span| match self.symbols.get(name) {
            Some(SymbolDef {
                value: SymbolValue::Constant(value),
                ..
            }) => Ok(*value),
            Some(SymbolDef {
                value: SymbolValue::Label(_),
                ..
            }) => Err(LineError::new(span, format!("«{name}» es una etiqueta; aquí hace falta una constante"))),
            None => Err(LineError::new(span, format!("constante no definida «{name}» (las constantes se definen con .equ antes de usarlas)"))),
        })
    }

    fn eval(
        &self,
        expr: &Expr,
        resolve: &dyn Fn(&str, Span) -> LineResult<i64>,
    ) -> LineResult<i64> {
        let overflow = || LineError::new(Span::default(), "la expresión se desborda");
        match expr {
            Expr::Number(value) => Ok(*value),
            Expr::Symbol(name, span) => resolve(name, *span),
            Expr::Negate(inner) => self
                .eval(inner, resolve)?
                .checked_neg()
                .ok_or_else(overflow),
            Expr::Add(left, right) => self
                .eval(left, resolve)?
                .checked_add(self.eval(right, resolve)?)
                .ok_or_else(overflow),
            Expr::Sub(left, right) => self
                .eval(left, resolve)?
                .checked_sub(self.eval(right, resolve)?)
                .ok_or_else(overflow),
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
}
