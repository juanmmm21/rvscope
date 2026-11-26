//! De tokens a sentencias: etiquetas, mnemónico o directiva y operandos.

use rv_isa::Reg;

use crate::error::{LineError, LineResult, Span};
use crate::lexer::{Token, TokenKind, tokenize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    /// `%hi(x)`: los 20 bits altos, compensando el signo de `%lo`.
    Hi,
    /// `%lo(x)`: los 12 bits bajos con signo.
    Lo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Shl,
    Shr,
    And,
    Or,
    Xor,
}

impl BinaryOp {
    /// Precedencia como en C, de menos a más: `|`, `^`, `&`, desplazamientos, suma y producto.
    fn precedence(self) -> u8 {
        match self {
            BinaryOp::Or => 1,
            BinaryOp::Xor => 2,
            BinaryOp::And => 3,
            BinaryOp::Shl | BinaryOp::Shr => 4,
            BinaryOp::Add | BinaryOp::Sub => 5,
            BinaryOp::Mul | BinaryOp::Div => 6,
        }
    }

    fn of(kind: &TokenKind) -> Option<BinaryOp> {
        Some(match kind {
            TokenKind::Plus => BinaryOp::Add,
            TokenKind::Minus => BinaryOp::Sub,
            TokenKind::Star => BinaryOp::Mul,
            TokenKind::Slash => BinaryOp::Div,
            TokenKind::Shl => BinaryOp::Shl,
            TokenKind::Shr => BinaryOp::Shr,
            TokenKind::Amp => BinaryOp::And,
            TokenKind::Pipe => BinaryOp::Or,
            TokenKind::Caret => BinaryOp::Xor,
            _ => return None,
        })
    }
}

/// Referencia a una etiqueta local numérica. `ordinal` dice a cuál de las definiciones de
/// `number` apunta (la primera es 1); lo fija el ensamblador al recorrer el fuente en orden,
/// porque `1b` y `1f` dependen de dónde está la referencia.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalRef {
    pub number: u32,
    pub forward: bool,
    pub ordinal: usize,
    pub span: Span,
}

impl LocalRef {
    pub fn text(&self) -> String {
        format!("{}{}", self.number, if self.forward { 'f' } else { 'b' })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Number(i64),
    Symbol(String, Span),
    Local(LocalRef),
    Negate(Box<Expr>),
    Not(Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Modified(Modifier, Box<Expr>),
}

impl Expr {
    /// Recorre las referencias locales para que el ensamblador las resuelva.
    pub fn locals_mut(&mut self, visit: &mut dyn FnMut(&mut LocalRef)) {
        match self {
            Expr::Local(local) => visit(local),
            Expr::Negate(inner) | Expr::Not(inner) | Expr::Modified(_, inner) => {
                inner.locals_mut(visit)
            }
            Expr::Binary(_, left, right) => {
                left.locals_mut(visit);
                right.locals_mut(visit);
            }
            Expr::Number(_) | Expr::Symbol(..) => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperandKind {
    Register(Reg),
    Expr(Expr),
    /// `offset(base)`; el desplazamiento puede omitirse.
    Memory {
        offset: Option<Expr>,
        base: Reg,
    },
    Str(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Operand {
    pub kind: OperandKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LabelName {
    Named(String),
    /// `1:`; puede repetirse y se distingue por orden de aparición.
    Local(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub name: LabelName,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statement {
    /// Mnemónico o directiva (con su punto), en minúsculas.
    pub name: String,
    pub name_span: Span,
    pub operands: Vec<Operand>,
}

impl Statement {
    pub fn locals_mut(&mut self, visit: &mut dyn FnMut(&mut LocalRef)) {
        for operand in &mut self.operands {
            match &mut operand.kind {
                OperandKind::Expr(expr)
                | OperandKind::Memory {
                    offset: Some(expr), ..
                } => expr.locals_mut(visit),
                OperandKind::Register(_) | OperandKind::Memory { offset: None, .. } => {}
                OperandKind::Str(_) => {}
            }
        }
    }

    pub fn is_directive(&self) -> bool {
        self.name.starts_with('.')
    }

    /// Tramo de toda la sentencia, del mnemónico al último operando.
    pub fn span(&self) -> Span {
        self.operands
            .last()
            .map_or(self.name_span, |last| self.name_span.to(last.span))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub labels: Vec<Label>,
    pub statement: Option<Statement>,
}

/// Analiza una línea de fuente. Con `;` puede llevar varias sentencias; las columnas de los
/// errores son siempre las de la línea completa.
pub fn parse_line(text: &str) -> LineResult<Vec<Line>> {
    let tokens = tokenize(text)?;
    tokens
        .split(|token| token.kind == TokenKind::Semicolon)
        .filter(|part| !part.is_empty())
        .map(parse_statement)
        .collect()
}

fn parse_statement(tokens: &[Token]) -> LineResult<Line> {
    let mut rest = tokens;
    let mut labels = Vec::new();
    loop {
        match rest {
            [
                Token {
                    kind: TokenKind::Ident(name),
                    span,
                },
                Token {
                    kind: TokenKind::Colon,
                    ..
                },
                tail @ ..,
            ] => {
                labels.push(Label {
                    name: LabelName::Named(name.clone()),
                    span: *span,
                });
                rest = tail;
            }
            [
                Token {
                    kind: TokenKind::LocalDef(number),
                    span,
                },
                tail @ ..,
            ] => {
                labels.push(Label {
                    name: LabelName::Local(*number),
                    span: *span,
                });
                rest = tail;
            }
            _ => break,
        }
    }
    let Some((head, tail)) = rest.split_first() else {
        return Ok(Line {
            labels,
            statement: None,
        });
    };
    let TokenKind::Ident(name) = &head.kind else {
        return Err(LineError::new(
            head.span,
            "se esperaba una instrucción, una directiva o una etiqueta",
        ));
    };
    let operands = split_operands(tail, head.span)?
        .into_iter()
        .map(parse_operand)
        .collect::<LineResult<Vec<_>>>()?;
    Ok(Line {
        labels,
        statement: Some(Statement {
            name: name.to_ascii_lowercase(),
            name_span: head.span,
            operands,
        }),
    })
}

fn split_operands(tokens: &[Token], name_span: Span) -> LineResult<Vec<&[Token]>> {
    if tokens.is_empty() {
        return Ok(Vec::new());
    }
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LParen => depth += 1,
            TokenKind::RParen => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| LineError::new(token.span, "paréntesis de cierre sin abrir"))?;
            }
            TokenKind::Comma if depth == 0 => {
                parts.push(&tokens[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        let last = tokens.last().map_or(name_span, |token| token.span);
        return Err(LineError::new(last, "falta un paréntesis de cierre"));
    }
    parts.push(&tokens[start..]);
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            // Sitúa el error en la coma que deja el operando vacío.
            let comma = tokens
                .iter()
                .filter(|token| token.kind == TokenKind::Comma)
                .nth(index.saturating_sub(1))
                .map_or(name_span, |token| token.span);
            return Err(LineError::new(comma, "falta un operando"));
        }
    }
    Ok(parts)
}

fn span_of(tokens: &[Token]) -> Span {
    match (tokens.first(), tokens.last()) {
        (Some(first), Some(last)) => first.span.to(last.span),
        _ => Span::default(),
    }
}

fn parse_operand(tokens: &[Token]) -> LineResult<Operand> {
    let span = span_of(tokens);
    let kind = match tokens {
        [
            Token {
                kind: TokenKind::Str(text),
                ..
            },
        ] => OperandKind::Str(text.clone()),
        [
            Token {
                kind: TokenKind::Ident(name),
                ..
            },
        ] if Reg::parse(name).is_some() => {
            OperandKind::Register(Reg::parse(name).unwrap_or_default())
        }
        [
            prefix @ ..,
            Token {
                kind: TokenKind::LParen,
                ..
            },
            Token {
                kind: TokenKind::Ident(base),
                ..
            },
            Token {
                kind: TokenKind::RParen,
                ..
            },
        ] if Reg::parse(base).is_some()
            && !matches!(
                prefix,
                [Token {
                    kind: TokenKind::Modifier(_),
                    ..
                }]
            ) =>
        {
            let offset = if prefix.is_empty() {
                None
            } else {
                Some(parse_expression(prefix)?)
            };
            OperandKind::Memory {
                offset,
                base: Reg::parse(base).unwrap_or_default(),
            }
        }
        _ => OperandKind::Expr(parse_expression(tokens)?),
    };
    Ok(Operand { kind, span })
}

fn parse_expression(tokens: &[Token]) -> LineResult<Expr> {
    let mut parser = ExprParser {
        tokens,
        position: 0,
    };
    let expr = parser.binary(0)?;
    match parser.tokens.get(parser.position) {
        None => Ok(expr),
        Some(token) => Err(LineError::new(token.span, "sobra algo en la expresión")),
    }
}

struct ExprParser<'a> {
    tokens: &'a [Token],
    position: usize,
}

impl ExprParser<'_> {
    fn next(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.position);
        if token.is_some() {
            self.position += 1;
        }
        token
    }

    fn end_span(&self) -> Span {
        self.tokens.last().map_or(Span::default(), |token| {
            Span::new(token.span.start + token.span.len, 1)
        })
    }

    /// Operadores binarios por precedencia ascendente, todos asociativos por la izquierda.
    fn binary(&mut self, min_precedence: u8) -> LineResult<Expr> {
        let mut left = self.unary()?;
        while let Some(op) = self
            .tokens
            .get(self.position)
            .and_then(|token| BinaryOp::of(&token.kind))
            .filter(|op| op.precedence() > min_precedence)
        {
            self.position += 1;
            let right = self.binary(op.precedence())?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn closing(&mut self) -> LineResult<()> {
        let end = self.end_span();
        match self.next().map(|t| t.kind.clone()) {
            Some(TokenKind::RParen) => Ok(()),
            _ => Err(LineError::new(end, "falta el paréntesis de cierre")),
        }
    }

    fn unary(&mut self) -> LineResult<Expr> {
        let end = self.end_span();
        let Some(token) = self.next().cloned() else {
            return Err(LineError::new(end, "falta un valor"));
        };
        match token.kind {
            TokenKind::Minus => Ok(Expr::Negate(Box::new(self.unary()?))),
            TokenKind::Tilde => Ok(Expr::Not(Box::new(self.unary()?))),
            TokenKind::Plus => self.unary(),
            TokenKind::LParen => {
                let inner = self.binary(0)?;
                self.closing()?;
                Ok(inner)
            }
            TokenKind::Int(value) => Ok(Expr::Number(value)),
            TokenKind::LocalRef { number, forward } => Ok(Expr::Local(LocalRef {
                number,
                forward,
                ordinal: 0,
                span: token.span,
            })),
            TokenKind::Ident(name) => {
                if Reg::parse(&name).is_some() {
                    return Err(LineError::new(
                        token.span,
                        format!("«{name}» es un registro; aquí se esperaba un valor"),
                    ));
                }
                Ok(Expr::Symbol(name, token.span))
            }
            TokenKind::Modifier(name) => {
                let modifier = match name.as_str() {
                    "hi" => Modifier::Hi,
                    "lo" => Modifier::Lo,
                    _ => {
                        return Err(LineError::new(
                            token.span,
                            format!("modificador desconocido «%{name}»; se admiten %hi y %lo"),
                        ));
                    }
                };
                let open = self.next().cloned();
                if !matches!(open.as_ref().map(|t| &t.kind), Some(TokenKind::LParen)) {
                    return Err(LineError::new(
                        token.span,
                        format!("«%{name}» va seguido de un valor entre paréntesis"),
                    ));
                }
                let inner = self.binary(0)?;
                self.closing()?;
                Ok(Expr::Modified(modifier, Box::new(inner)))
            }
            _ => Err(LineError::new(
                token.span,
                "se esperaba un número o una etiqueta",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statement(text: &str) -> Statement {
        let mut lines = parse_line(text).unwrap();
        assert_eq!(lines.len(), 1);
        lines.remove(0).statement.unwrap()
    }

    fn expression(text: &str) -> Expr {
        match statement(&format!(".word {text}")).operands.remove(0).kind {
            OperandKind::Expr(expr) => expr,
            other => panic!("no es una expresión: {other:?}"),
        }
    }

    fn binary(op: BinaryOp, left: Expr, right: Expr) -> Expr {
        Expr::Binary(op, Box::new(left), Box::new(right))
    }

    fn reg(name: &str) -> Reg {
        Reg::parse(name).unwrap()
    }

    #[test]
    fn parses_labels_and_empty_lines() {
        let lines = parse_line("a: 1: b: # nada").unwrap();
        assert_eq!(
            lines[0]
                .labels
                .iter()
                .map(|l| l.name.clone())
                .collect::<Vec<_>>(),
            [
                LabelName::Named("a".into()),
                LabelName::Local(1),
                LabelName::Named("b".into())
            ]
        );
        assert_eq!(lines[0].statement, None);
        assert_eq!(parse_line("   ").unwrap(), Vec::new());
        assert_eq!(parse_line(" ;; ").unwrap(), Vec::new());
    }

    #[test]
    fn parses_register_memory_and_expression_operands() {
        let stmt = statement("SW a0, -4(sp)");
        assert_eq!(stmt.name, "sw");
        assert_eq!(stmt.operands[0].kind, OperandKind::Register(reg("a0")));
        assert_eq!(
            stmt.operands[1].kind,
            OperandKind::Memory {
                offset: Some(Expr::Negate(Box::new(Expr::Number(4)))),
                base: reg("sp")
            }
        );
        let stmt = statement("lw t0, (t1)");
        assert_eq!(
            stmt.operands[1].kind,
            OperandKind::Memory {
                offset: None,
                base: reg("t1")
            }
        );
        let stmt = statement("addi a0, a0, %lo(msg+4)");
        assert!(matches!(
            &stmt.operands[2].kind,
            OperandKind::Expr(Expr::Modified(Modifier::Lo, _))
        ));
        let stmt = statement("la a0, msg + 4");
        assert!(matches!(
            &stmt.operands[1].kind,
            OperandKind::Expr(Expr::Binary(BinaryOp::Add, ..))
        ));
        let stmt = statement("lw a0, %lo(msg)(a1)");
        assert!(matches!(
            &stmt.operands[1].kind,
            OperandKind::Memory {
                offset: Some(Expr::Modified(Modifier::Lo, _)),
                ..
            }
        ));
    }

    #[test]
    fn rejects_malformed_operands() {
        assert_eq!(
            parse_line("add a0,, a1").unwrap_err().message,
            "falta un operando"
        );
        assert!(parse_line("add a0, a1,").is_err());
        assert!(parse_line("lw a0, 4(sp").is_err());
        assert!(parse_line("addi a0, a0, 1 2").is_err());
        assert!(parse_line("addi a0, a0, a1 + 1").is_err());
        assert!(parse_line("lui a0, %pcrel_hi(x)").is_err());
        assert!(parse_line("123 a0").is_err());
        assert!(parse_line("li a0, (1 + 2").is_err());
        assert!(parse_line("li a0, 1 +").is_err());
    }

    #[test]
    fn semicolons_split_statements_keeping_columns() {
        let lines = parse_line("x: li a0, 1; 1: addi a0, a0, -1;; bnez a0, 1b").unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1].labels[0].name, LabelName::Local(1));
        let branch = lines[2].statement.as_ref().unwrap();
        assert_eq!(branch.name_span.start, 34);
        assert!(matches!(
            branch.operands[1].kind,
            OperandKind::Expr(Expr::Local(LocalRef {
                number: 1,
                forward: false,
                ..
            }))
        ));
    }

    #[test]
    fn expressions_follow_c_precedence() {
        let n = Expr::Number;
        assert_eq!(
            expression("1 + 2 * 3"),
            binary(BinaryOp::Add, n(1), binary(BinaryOp::Mul, n(2), n(3)))
        );
        assert_eq!(
            expression("8 - 4 - 2"),
            binary(BinaryOp::Sub, binary(BinaryOp::Sub, n(8), n(4)), n(2))
        );
        assert_eq!(
            expression("1 | 2 & 3 << 4"),
            binary(
                BinaryOp::Or,
                n(1),
                binary(BinaryOp::And, n(2), binary(BinaryOp::Shl, n(3), n(4)))
            )
        );
        assert_eq!(
            expression("(1 + 2) * ~3"),
            binary(
                BinaryOp::Mul,
                binary(BinaryOp::Add, n(1), n(2)),
                Expr::Not(Box::new(n(3)))
            )
        );
        // Un valor entre paréntesis seguido de un registro sigue siendo un acceso a memoria.
        let stmt = statement("lw a0, (4 * 2)(sp)");
        assert!(matches!(
            stmt.operands[1].kind,
            OperandKind::Memory {
                offset: Some(Expr::Binary(BinaryOp::Mul, ..)),
                ..
            }
        ));
    }

    #[test]
    fn statement_span_covers_operands() {
        let stmt = statement("  add a0, a1, a2");
        assert_eq!(stmt.span(), Span::new(2, 14));
    }
}
