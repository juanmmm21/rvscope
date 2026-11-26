//! Ensamblador de RISC-V (RV32IM) de rvscope.
//!
//! Admite etiquetas, directivas de datos y secciones, constantes con `.equ`, `%hi`/`%lo` y las
//! pseudoinstrucciones de la tabla de `rv-isa`. Un programa mal escrito nunca provoca un pánico:
//! devuelve la lista de errores con línea y columna.

mod assembler;
mod directives;
mod error;
mod eval;
mod expand;
mod lexer;
mod parser;
mod program;

pub use assembler::{assemble, assemble_with};
pub use directives::{DIRECTIVES, DirectiveSpec, MODIFIERS};
pub use error::{AsmError, Span};
pub use program::{Layout, ListingEntry, Program, Segment};
