//! Directivas y modificadores que admite el ensamblador, para el resaltado del editor y la
//! chuleta. Un test ensambla el `example` de cada directiva: la lista no puede anunciar una que
//! el ensamblador rechace.

use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct DirectiveSpec {
    pub name: &'static str,
    pub syntax: &'static str,
    pub summary: &'static str,
    /// Uso que el ensamblador debe aceptar.
    pub example: &'static str,
}

const fn directive(
    name: &'static str,
    syntax: &'static str,
    summary: &'static str,
    example: &'static str,
) -> DirectiveSpec {
    DirectiveSpec {
        name,
        syntax,
        summary,
        example,
    }
}

#[rustfmt::skip]
pub static DIRECTIVES: [DirectiveSpec; 23] = [
    directive(".text", ".text", "Lo que sigue es código", ".text"),
    directive(".data", ".data", "Lo que sigue son datos", ".data"),
    directive(".rodata", ".rodata", "Datos de solo lectura (van con los datos)", ".rodata"),
    directive(".bss", ".bss", "Datos a cero (van con los datos)", ".bss"),
    directive(".section", ".section .text | .data", "Cambia de sección", ".section .data"),
    directive(".globl", ".globl símbolo", "Marca un símbolo como global (no cambia nada)", ".globl main"),
    directive(".global", ".global símbolo", "Igual que .globl", ".global main"),
    directive(".equ", ".equ NOMBRE, valor", "Define una constante", ".equ SIZE, 16"),
    directive(".set", ".set NOMBRE, valor", "Igual que .equ", ".set SIZE, 16"),
    directive(".align", ".align n", "Alinea a 2^n bytes", ".align 2"),
    directive(".p2align", ".p2align n", "Igual que .align", ".p2align 2"),
    directive(".balign", ".balign bytes", "Alinea a un número de bytes potencia de dos", ".balign 8"),
    directive(".byte", ".byte v, ...", "Bytes", ".byte 1, 2, 'a'"),
    directive(".half", ".half v, ...", "Medias palabras de 16 bits", ".half 1, -1"),
    directive(".short", ".short v, ...", "Igual que .half", ".short 1, -1"),
    directive(".word", ".word v, ...", "Palabras de 32 bits", ".word 1, 0x10, label"),
    directive(".long", ".long v, ...", "Igual que .word", ".long 7"),
    directive(".string", ".string \"texto\"", "Cadena terminada en cero", ".string \"hola\""),
    directive(".asciz", ".asciz \"texto\"", "Igual que .string", ".asciz \"hola\""),
    directive(".ascii", ".ascii \"texto\"", "Cadena sin terminador", ".ascii \"hola\""),
    directive(".space", ".space bytes", "Reserva bytes a cero", ".space 8"),
    directive(".zero", ".zero bytes", "Igual que .space", ".zero 8"),
    directive(".option", ".option nombre", "Opción del ensamblador (se acepta y no hace nada)", ".option norvc"),
];

/// Modificadores de reubicación que se escriben como `%hi(expr)`.
pub static MODIFIERS: [&str; 2] = ["hi", "lo"];
