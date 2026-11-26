//! Traducción de una sentencia (instrucción real o pseudoinstrucción) a instrucciones reales.

use rv_isa::{Instruction, Mnemonic, Operands, PSEUDO_INSTRUCTIONS, Reg, find};

use crate::error::{LineError, LineResult, Span};
use crate::eval::{Context, hi20, lo12, wrap32};
use crate::parser::{Expr, Operand, OperandKind, Statement};

/// Instrucción real producida por una sentencia, con el tramo al que culpar si no se puede
/// codificar (normalmente el operando del que sale el inmediato).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Emitted {
    pub instruction: Instruction,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expansion {
    pub emitted: Vec<Emitted>,
    /// Nombre de la pseudoinstrucción de la que sale, si no es una instrucción real.
    pub pseudo: Option<&'static str>,
}

/// Formas que admite un nombre, para los mensajes de error («addi rd, rs1, imm»).
fn usage(name: &str) -> String {
    let mut forms: Vec<String> = find(name).map(|spec| spec.syntax()).into_iter().collect();
    forms.extend(
        PSEUDO_INSTRUCTIONS
            .iter()
            .filter(|pseudo| pseudo.name == name)
            .map(|pseudo| pseudo.syntax.to_owned()),
    );
    forms.join("  o  ")
}

fn is_pseudo(name: &str) -> Option<&'static str> {
    PSEUDO_INSTRUCTIONS
        .iter()
        .find(|pseudo| pseudo.name == name)
        .map(|pseudo| pseudo.name)
}

fn reg(operand: &Operand) -> LineResult<Reg> {
    match &operand.kind {
        OperandKind::Register(reg) => Ok(*reg),
        _ => Err(LineError::new(operand.span, "se esperaba un registro")),
    }
}

fn expr(operand: &Operand) -> LineResult<&Expr> {
    match &operand.kind {
        OperandKind::Expr(expr) => Ok(expr),
        OperandKind::Register(_) => Err(LineError::new(
            operand.span,
            "se esperaba un valor o una etiqueta, no un registro",
        )),
        _ => Err(LineError::new(
            operand.span,
            "se esperaba un valor o una etiqueta",
        )),
    }
}

fn to_i32(value: i64, span: Span) -> LineResult<i32> {
    i32::try_from(value)
        .map_err(|_| LineError::new(span, format!("el valor {value} no cabe en el inmediato")))
}

fn address(value: i64, span: Span) -> LineResult<u32> {
    u32::try_from(value)
        .map_err(|_| LineError::new(span, format!("{value} no es una dirección válida")))
}

struct Builder<'a> {
    statement: &'a Statement,
    context: &'a Context<'a>,
    pc: u32,
    emitted: Vec<Emitted>,
}

impl<'a> Builder<'a> {
    fn emit(&mut self, mnemonic: Mnemonic, rd: Reg, rs1: Reg, rs2: Reg, imm: i32, span: Span) {
        self.emitted.push(Emitted {
            instruction: Instruction::new(mnemonic, rd, rs1, rs2, imm),
            span,
        });
    }

    /// Dirección de la siguiente instrucción que se emita.
    fn current_pc(&self) -> u32 {
        let emitted = u32::try_from(self.emitted.len()).unwrap_or(u32::MAX);
        self.pc.wrapping_add(emitted.wrapping_mul(4))
    }

    /// Desplazamiento desde la siguiente instrucción hasta la etiqueta del operando.
    fn offset_to(&self, operand: &Operand) -> LineResult<i32> {
        let target = address(self.context.value(expr(operand)?)?, operand.span)?;
        to_i32(
            i64::from(target) - i64::from(self.current_pc()),
            operand.span,
        )
    }

    fn arity(&self, expected: usize) -> LineResult<&'a [Operand]> {
        let statement: &'a Statement = self.statement;
        let operands = &statement.operands;
        if operands.len() == expected {
            Ok(operands)
        } else {
            Err(self.wrong_form())
        }
    }

    fn wrong_form(&self) -> LineError {
        let name = &self.statement.name;
        LineError::new(
            self.statement.span(),
            format!("operandos no válidos; se usa así: {}", usage(name)),
        )
    }

    /// `auipc` + instrucción con `%pcrel_lo`, para llegar a cualquier dirección.
    fn pc_relative(
        &mut self,
        target_operand: &Operand,
        auipc_rd: Reg,
        second: Mnemonic,
        rd: Reg,
        rs2: Reg,
    ) -> LineResult<()> {
        let target = address(
            self.context.value(expr(target_operand)?)?,
            target_operand.span,
        )?;
        let delta = target.wrapping_sub(self.current_pc());
        let span = target_operand.span;
        self.emit(
            Mnemonic::Auipc,
            auipc_rd,
            Reg::ZERO,
            Reg::ZERO,
            (hi20(delta) << 12) as i32,
            span,
        );
        self.emit(second, rd, auipc_rd, rs2, lo12(delta), span);
        Ok(())
    }

    fn real(&mut self, mnemonic: Mnemonic) -> LineResult<Form> {
        let spec = mnemonic.spec();
        let statement: &'a Statement = self.statement;
        let operands = &statement.operands;
        match spec.operands {
            Operands::RdRs1Rs2 => {
                let ops = self.arity(3)?;
                let (rd, rs1) = (reg(&ops[0])?, reg(&ops[1])?);
                // Como gas, `add a0, a1, 5` es `addi`: el mnemónico con inmediato existe y no
                // hay forma de confundirlo con otra cosa.
                if let (OperandKind::Expr(value), Some(immediate)) =
                    (&ops[2].kind, immediate_form(mnemonic))
                {
                    let imm = to_i32(self.context.value(value)?, ops[2].span)?;
                    self.emit(immediate, rd, rs1, Reg::ZERO, imm, ops[2].span);
                } else {
                    let rs2 = reg(&ops[2])?;
                    self.emit(mnemonic, rd, rs1, rs2, 0, self.statement.span());
                }
            }
            Operands::RdRs1Imm | Operands::RdRs1Shamt => {
                let ops = self.arity(3)?;
                let (rd, rs1) = (reg(&ops[0])?, reg(&ops[1])?);
                let imm = to_i32(self.context.value(expr(&ops[2])?)?, ops[2].span)?;
                self.emit(mnemonic, rd, rs1, Reg::ZERO, imm, ops[2].span);
            }
            Operands::RdOffsetRs1 => match operands.as_slice() {
                [
                    rd,
                    Operand {
                        kind: OperandKind::Memory { offset, base },
                        span,
                    },
                ] => {
                    let imm = self.memory_offset(offset.as_ref(), *span)?;
                    self.emit(mnemonic, reg(rd)?, *base, Reg::ZERO, imm, *span);
                }
                // jalr rd, rs1, imm  y  jalr rd, rs1
                [rd, rs1, rest @ ..] if mnemonic == Mnemonic::Jalr && rest.len() <= 1 => {
                    let (imm, span) = match rest.first() {
                        Some(op) => (to_i32(self.context.value(expr(op)?)?, op.span)?, op.span),
                        None => (0, rs1.span),
                    };
                    self.emit(mnemonic, reg(rd)?, reg(rs1)?, Reg::ZERO, imm, span);
                }
                // jalr rs
                [rs] if mnemonic == Mnemonic::Jalr => {
                    self.emit(mnemonic, Reg::RA, reg(rs)?, Reg::ZERO, 0, rs.span);
                    return Ok(Form::Pseudo);
                }
                // lw rd, etiqueta
                [
                    rd,
                    target @ Operand {
                        kind: OperandKind::Expr(_),
                        ..
                    },
                ] => {
                    let rd = reg(rd)?;
                    self.pc_relative(target, rd, mnemonic, rd, Reg::ZERO)?;
                    return Ok(Form::Pseudo);
                }
                _ => return Err(self.wrong_form()),
            },
            Operands::Rs2OffsetRs1 => match operands.as_slice() {
                [
                    rs2,
                    Operand {
                        kind: OperandKind::Memory { offset, base },
                        span,
                    },
                ] => {
                    let imm = self.memory_offset(offset.as_ref(), *span)?;
                    self.emit(mnemonic, Reg::ZERO, *base, reg(rs2)?, imm, *span);
                }
                // sw rs, etiqueta, rt
                [rs2, target, temp] => {
                    let (rs2, temp) = (reg(rs2)?, reg(temp)?);
                    self.pc_relative(target, temp, mnemonic, Reg::ZERO, rs2)?;
                    return Ok(Form::Pseudo);
                }
                _ => return Err(self.wrong_form()),
            },
            Operands::Rs1Rs2Target => {
                let ops = self.arity(3)?;
                let (rs1, rs2) = (reg(&ops[0])?, reg(&ops[1])?);
                let offset = self.offset_to(&ops[2])?;
                self.emit(mnemonic, Reg::ZERO, rs1, rs2, offset, ops[2].span);
            }
            Operands::RdUpper => {
                let ops = self.arity(2)?;
                let rd = reg(&ops[0])?;
                let value = self.context.value(expr(&ops[1])?)?;
                if !(0..=0xf_ffff).contains(&value) {
                    return Err(LineError::new(
                        ops[1].span,
                        format!(
                            "el inmediato de {} va de 0 a 0xFFFFF (20 bits); vale {value}",
                            spec.name
                        ),
                    ));
                }
                self.emit(
                    mnemonic,
                    rd,
                    Reg::ZERO,
                    Reg::ZERO,
                    (wrap32(value) << 12) as i32,
                    ops[1].span,
                );
            }
            Operands::RdTarget => match operands.as_slice() {
                [rd, target] => {
                    let rd = reg(rd)?;
                    let offset = self.offset_to(target)?;
                    self.emit(mnemonic, rd, Reg::ZERO, Reg::ZERO, offset, target.span);
                }
                // jal etiqueta
                [target] => {
                    let offset = self.offset_to(target)?;
                    self.emit(mnemonic, Reg::RA, Reg::ZERO, Reg::ZERO, offset, target.span);
                    return Ok(Form::Pseudo);
                }
                _ => return Err(self.wrong_form()),
            },
            Operands::Fence => {
                let imm = match operands.as_slice() {
                    [] => 0xff,
                    [pred, succ] => (fence_set(pred)? << 4) | fence_set(succ)?,
                    _ => return Err(self.wrong_form()),
                };
                self.emit(
                    mnemonic,
                    Reg::ZERO,
                    Reg::ZERO,
                    Reg::ZERO,
                    imm,
                    self.statement.span(),
                );
            }
            Operands::None => {
                self.arity(0)?;
                self.emit(
                    mnemonic,
                    Reg::ZERO,
                    Reg::ZERO,
                    Reg::ZERO,
                    0,
                    self.statement.span(),
                );
            }
        }
        Ok(Form::Real)
    }

    fn memory_offset(&self, offset: Option<&Expr>, span: Span) -> LineResult<i32> {
        match offset {
            Some(expr) => to_i32(self.context.value(expr)?, span),
            None => Ok(0),
        }
    }

    fn pseudo(&mut self, name: &str) -> LineResult<()> {
        use Mnemonic as M;
        let span = self.statement.span();
        match name {
            "nop" => {
                self.arity(0)?;
                self.emit(M::Addi, Reg::ZERO, Reg::ZERO, Reg::ZERO, 0, span);
            }
            "ret" => {
                self.arity(0)?;
                self.emit(M::Jalr, Reg::ZERO, Reg::RA, Reg::ZERO, 0, span);
            }
            "li" => {
                let ops = self.arity(2)?;
                let rd = reg(&ops[0])?;
                let value = self.context.constant(expr(&ops[1])?)?;
                if !(i64::from(i32::MIN)..=i64::from(u32::MAX)).contains(&value) {
                    return Err(LineError::new(
                        ops[1].span,
                        format!("{value} no cabe en 32 bits"),
                    ));
                }
                let bits = wrap32(value);
                let small = bits as i32;
                if (-2048..=2047).contains(&small) {
                    self.emit(M::Addi, rd, Reg::ZERO, Reg::ZERO, small, ops[1].span);
                } else {
                    self.emit(
                        M::Lui,
                        rd,
                        Reg::ZERO,
                        Reg::ZERO,
                        (hi20(bits) << 12) as i32,
                        ops[1].span,
                    );
                    let low = lo12(bits);
                    if low != 0 {
                        self.emit(M::Addi, rd, rd, Reg::ZERO, low, ops[1].span);
                    }
                }
            }
            "la" | "lla" => {
                let ops = self.arity(2)?;
                let rd = reg(&ops[0])?;
                self.pc_relative(&ops[1], rd, M::Addi, rd, Reg::ZERO)?;
            }
            "mv" | "not" | "neg" | "seqz" | "snez" | "sltz" | "sgtz" => {
                let ops = self.arity(2)?;
                let (rd, rs) = (reg(&ops[0])?, reg(&ops[1])?);
                let z = Reg::ZERO;
                let (mnemonic, rs1, rs2, imm) = match name {
                    "mv" => (M::Addi, rs, z, 0),
                    "not" => (M::Xori, rs, z, -1),
                    "neg" => (M::Sub, z, rs, 0),
                    "seqz" => (M::Sltiu, rs, z, 1),
                    "snez" => (M::Sltu, z, rs, 0),
                    "sltz" => (M::Slt, rs, z, 0),
                    _ => (M::Slt, z, rs, 0),
                };
                self.emit(mnemonic, rd, rs1, rs2, imm, span);
            }
            "beqz" | "bnez" | "blez" | "bgez" | "bltz" | "bgtz" => {
                let ops = self.arity(2)?;
                let rs = reg(&ops[0])?;
                let offset = self.offset_to(&ops[1])?;
                let z = Reg::ZERO;
                let (mnemonic, rs1, rs2) = match name {
                    "beqz" => (M::Beq, rs, z),
                    "bnez" => (M::Bne, rs, z),
                    "blez" => (M::Bge, z, rs),
                    "bgez" => (M::Bge, rs, z),
                    "bltz" => (M::Blt, rs, z),
                    _ => (M::Blt, z, rs),
                };
                self.emit(mnemonic, z, rs1, rs2, offset, ops[1].span);
            }
            "bgt" | "ble" | "bgtu" | "bleu" => {
                let ops = self.arity(3)?;
                let (rs, rt) = (reg(&ops[0])?, reg(&ops[1])?);
                let offset = self.offset_to(&ops[2])?;
                // Se intercambian los operandos: rs > rt equivale a rt < rs.
                let mnemonic = match name {
                    "bgt" => M::Blt,
                    "ble" => M::Bge,
                    "bgtu" => M::Bltu,
                    _ => M::Bgeu,
                };
                self.emit(mnemonic, Reg::ZERO, rt, rs, offset, ops[2].span);
            }
            "j" => {
                let ops = self.arity(1)?;
                let offset = self.offset_to(&ops[0])?;
                self.emit(M::Jal, Reg::ZERO, Reg::ZERO, Reg::ZERO, offset, ops[0].span);
            }
            "jr" => {
                let ops = self.statement.operands.as_slice();
                let (rs, imm, span) = match ops {
                    [rs] => (reg(rs)?, 0, rs.span),
                    [rs, offset] => (
                        reg(rs)?,
                        to_i32(self.context.value(expr(offset)?)?, offset.span)?,
                        offset.span,
                    ),
                    _ => return Err(self.wrong_form()),
                };
                self.emit(M::Jalr, Reg::ZERO, rs, Reg::ZERO, imm, span);
            }
            "call" | "tail" => {
                let ops = self.arity(1)?;
                let (link, temp) = if name == "call" {
                    (Reg::RA, Reg::RA)
                } else {
                    (Reg::ZERO, Reg::T1)
                };
                self.pc_relative(&ops[0], temp, M::Jalr, link, Reg::ZERO)?;
            }
            _ => {
                return Err(LineError::new(
                    span,
                    format!(
                        "operandos no válidos para «{name}»; se usa así: {}",
                        usage(name)
                    ),
                ));
            }
        }
        Ok(())
    }
}

/// Versión con inmediato de una instrucción de registro a registro, si la hay.
fn immediate_form(mnemonic: Mnemonic) -> Option<Mnemonic> {
    use Mnemonic as M;
    Some(match mnemonic {
        M::Add => M::Addi,
        M::Slt => M::Slti,
        M::Sltu => M::Sltiu,
        M::Xor => M::Xori,
        M::Or => M::Ori,
        M::And => M::Andi,
        M::Sll => M::Slli,
        M::Srl => M::Srli,
        M::Sra => M::Srai,
        _ => return None,
    })
}

/// Algunas instrucciones reales tienen además una forma de pseudoinstrucción con el mismo
/// nombre (`jal etiqueta`, `jalr rs`, `lw a0, etiqueta`, `sw a0, etiqueta, t0`).
enum Form {
    Real,
    Pseudo,
}

fn fence_set(operand: &Operand) -> LineResult<i32> {
    let invalid = || {
        LineError::new(
            operand.span,
            "conjunto de fence no válido; usa letras de «iorw», p. ej. «rw»",
        )
    };
    let OperandKind::Expr(Expr::Symbol(text, _)) = &operand.kind else {
        return Err(invalid());
    };
    let mut bits = 0;
    for letter in text.chars() {
        let bit = match letter.to_ascii_lowercase() {
            'i' => 8,
            'o' => 4,
            'r' => 2,
            'w' => 1,
            _ => return Err(invalid()),
        };
        if bits & bit != 0 {
            return Err(invalid());
        }
        bits |= bit;
    }
    Ok(bits)
}

/// Expande una sentencia de instrucción situada en `pc`.
pub fn expand(statement: &Statement, pc: u32, context: &Context<'_>) -> LineResult<Expansion> {
    let mut builder = Builder {
        statement,
        context,
        pc,
        emitted: Vec::new(),
    };
    let name = statement.name.as_str();
    if let Some(spec) = find(name) {
        let pseudo = match builder.real(spec.mnemonic)? {
            Form::Real => None,
            Form::Pseudo => is_pseudo(name),
        };
        return Ok(Expansion {
            emitted: builder.emitted,
            pseudo,
        });
    }
    if let Some(pseudo) = is_pseudo(name) {
        builder.pseudo(pseudo)?;
        return Ok(Expansion {
            emitted: builder.emitted,
            pseudo: Some(pseudo),
        });
    }
    let hint = if name == "fence.i" || name.starts_with("csr") {
        " (esa extensión no está admitida)"
    } else {
        ""
    };
    Err(LineError::new(
        statement.name_span,
        format!("instrucción desconocida «{name}»{hint}"),
    ))
}
