//! Pseudoinstrucciones: lo que se escribe y en qué instrucciones reales se convierte.
//!
//! El ensamblador tiene un test que ensambla el `example` de cada entrada, así que esta lista
//! no puede anunciar una pseudoinstrucción que no exista.

use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct PseudoSpec {
    pub name: &'static str,
    pub syntax: &'static str,
    /// Instrucciones reales en que se expande.
    pub expansion: &'static str,
    pub summary: &'static str,
    /// Uso concreto que el ensamblador debe aceptar (con una etiqueta `label` definida).
    pub example: &'static str,
}

const fn pseudo(
    name: &'static str,
    syntax: &'static str,
    expansion: &'static str,
    summary: &'static str,
    example: &'static str,
) -> PseudoSpec {
    PseudoSpec {
        name,
        syntax,
        expansion,
        summary,
        example,
    }
}

#[rustfmt::skip]
pub static PSEUDO_INSTRUCTIONS: [PseudoSpec; 36] = [
    pseudo("nop", "nop", "addi x0, x0, 0", "No hace nada", "nop"),
    pseudo("li", "li rd, imm", "addi rd, x0, imm  ·  o bien  ·  lui rd, %hi(imm); addi rd, rd, %lo(imm)",
        "Carga una constante de 32 bits", "li a0, 100000"),
    pseudo("la", "la rd, symbol", "auipc rd, %pcrel_hi(symbol); addi rd, rd, %pcrel_lo(symbol)",
        "Carga la dirección de una etiqueta", "la a0, label"),
    pseudo("lla", "lla rd, symbol", "auipc rd, %pcrel_hi(symbol); addi rd, rd, %pcrel_lo(symbol)",
        "Igual que la: dirección relativa al pc", "lla a0, label"),
    pseudo("mv", "mv rd, rs", "addi rd, rs, 0", "Copia un registro", "mv a0, a1"),
    pseudo("not", "not rd, rs", "xori rd, rs, -1", "Complemento a uno", "not a0, a1"),
    pseudo("neg", "neg rd, rs", "sub rd, x0, rs", "Cambia el signo", "neg a0, a1"),
    pseudo("seqz", "seqz rd, rs", "sltiu rd, rs, 1", "rd = 1 si rs es cero", "seqz a0, a1"),
    pseudo("snez", "snez rd, rs", "sltu rd, x0, rs", "rd = 1 si rs no es cero", "snez a0, a1"),
    pseudo("sltz", "sltz rd, rs", "slt rd, rs, x0", "rd = 1 si rs es negativo", "sltz a0, a1"),
    pseudo("sgtz", "sgtz rd, rs", "slt rd, x0, rs", "rd = 1 si rs es positivo", "sgtz a0, a1"),
    pseudo("beqz", "beqz rs, label", "beq rs, x0, label", "Salta si rs es cero", "beqz a0, label"),
    pseudo("bnez", "bnez rs, label", "bne rs, x0, label", "Salta si rs no es cero", "bnez a0, label"),
    pseudo("blez", "blez rs, label", "bge x0, rs, label", "Salta si rs <= 0", "blez a0, label"),
    pseudo("bgez", "bgez rs, label", "bge rs, x0, label", "Salta si rs >= 0", "bgez a0, label"),
    pseudo("bltz", "bltz rs, label", "blt rs, x0, label", "Salta si rs < 0", "bltz a0, label"),
    pseudo("bgtz", "bgtz rs, label", "blt x0, rs, label", "Salta si rs > 0", "bgtz a0, label"),
    pseudo("bgt", "bgt rs, rt, label", "blt rt, rs, label", "Salta si rs > rt con signo", "bgt a0, a1, label"),
    pseudo("ble", "ble rs, rt, label", "bge rt, rs, label", "Salta si rs <= rt con signo", "ble a0, a1, label"),
    pseudo("bgtu", "bgtu rs, rt, label", "bltu rt, rs, label", "Salta si rs > rt sin signo", "bgtu a0, a1, label"),
    pseudo("bleu", "bleu rs, rt, label", "bgeu rt, rs, label", "Salta si rs <= rt sin signo", "bleu a0, a1, label"),
    pseudo("j", "j label", "jal x0, label", "Salta sin guardar la vuelta", "j label"),
    pseudo("jal", "jal label", "jal ra, label", "Llama a una función cercana", "jal label"),
    pseudo("jr", "jr rs[, imm]", "jalr x0, imm(rs)", "Salta a la dirección de rs (más imm)", "jr t0, 4"),
    pseudo("jalr", "jalr rs", "jalr ra, 0(rs)", "Llama a la función cuya dirección está en rs", "jalr t0"),
    pseudo("ret", "ret", "jalr x0, 0(ra)", "Vuelve de una función", "ret"),
    pseudo("call", "call label", "auipc ra, %pcrel_hi(label); jalr ra, %pcrel_lo(label)(ra)",
        "Llama a una función a cualquier distancia", "call label"),
    pseudo("tail", "tail label", "auipc t1, %pcrel_hi(label); jalr x0, %pcrel_lo(label)(t1)",
        "Salta a una función sin volver aquí", "tail label"),
    pseudo("lb", "lb rd, symbol", "auipc rd, %pcrel_hi(symbol); lb rd, %pcrel_lo(symbol)(rd)",
        "Carga un byte de una etiqueta", "lb a0, label"),
    pseudo("lh", "lh rd, symbol", "auipc rd, %pcrel_hi(symbol); lh rd, %pcrel_lo(symbol)(rd)",
        "Carga media palabra de una etiqueta", "lh a0, label"),
    pseudo("lw", "lw rd, symbol", "auipc rd, %pcrel_hi(symbol); lw rd, %pcrel_lo(symbol)(rd)",
        "Carga una palabra de una etiqueta", "lw a0, label"),
    pseudo("lbu", "lbu rd, symbol", "auipc rd, %pcrel_hi(symbol); lbu rd, %pcrel_lo(symbol)(rd)",
        "Carga un byte sin signo de una etiqueta", "lbu a0, label"),
    pseudo("lhu", "lhu rd, symbol", "auipc rd, %pcrel_hi(symbol); lhu rd, %pcrel_lo(symbol)(rd)",
        "Carga media palabra sin signo de una etiqueta", "lhu a0, label"),
    pseudo("sb", "sb rs, symbol, rt", "auipc rt, %pcrel_hi(symbol); sb rs, %pcrel_lo(symbol)(rt)",
        "Guarda un byte en una etiqueta usando rt de apoyo", "sb a0, label, t0"),
    pseudo("sh", "sh rs, symbol, rt", "auipc rt, %pcrel_hi(symbol); sh rs, %pcrel_lo(symbol)(rt)",
        "Guarda media palabra en una etiqueta usando rt de apoyo", "sh a0, label, t0"),
    pseudo("sw", "sw rs, symbol, rt", "auipc rt, %pcrel_hi(symbol); sw rs, %pcrel_lo(symbol)(rt)",
        "Guarda una palabra en una etiqueta usando rt de apoyo", "sw a0, label, t0"),
];
