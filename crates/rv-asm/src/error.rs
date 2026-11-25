//! Errores del ensamblador, siempre con posición para marcarlos en el editor.

use serde::Serialize;
use thiserror::Error;

/// Tramo de una línea: columna inicial (desde 0, en caracteres) y longitud.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Span {
    pub start: usize,
    pub len: usize,
}

impl Span {
    pub fn new(start: usize, len: usize) -> Span {
        Span { start, len }
    }

    /// Tramo que va del inicio de `self` al final de `other`.
    pub fn to(self, other: Span) -> Span {
        let end = (other.start + other.len).max(self.start + self.len);
        Span::new(self.start, end - self.start)
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize)]
#[error("línea {line}, columna {column}: {message}")]
pub struct AsmError {
    /// Línea desde 1.
    pub line: usize,
    /// Columna desde 1.
    pub column: usize,
    /// Caracteres que abarca el error (al menos 1).
    pub length: usize,
    pub message: String,
}

impl AsmError {
    pub fn at(line: usize, span: Span, message: impl Into<String>) -> AsmError {
        AsmError {
            line,
            column: span.start + 1,
            length: span.len.max(1),
            message: message.into(),
        }
    }
}

/// Error sin línea todavía: lo produce el análisis de una línea y lo completa quien la recorre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineError {
    pub span: Span,
    pub message: String,
}

impl LineError {
    pub fn new(span: Span, message: impl Into<String>) -> LineError {
        LineError {
            span,
            message: message.into(),
        }
    }

    pub fn on_line(self, line: usize) -> AsmError {
        AsmError::at(line, self.span, self.message)
    }
}

pub type LineResult<T> = Result<T, LineError>;
