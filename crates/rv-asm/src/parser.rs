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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Number(i64),
    Symbol(String, Span),
    Negate(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Modified(Modifier, Box<Expr>),
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
pub struct Label {
    pub name: String,
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

pub fn parse_line(text: &str) -> LineResult<Line> {
    let tokens = tokenize(text)?;
    let mut rest: &[Token] = &tokens;
    let mut labels = Vec::new();
    while let [
        Token {
            kind: TokenKind::Ident(name),
            span,
        },
        Token {
            kind: TokenKind::Colon,
            ..
        },
        tail @ ..,
    ] = rest
    {
        labels.push(Label {
            name: name.clone(),
            span: *span,
        });
        rest = tail;
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
    let expr = parser.sum()?;
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

    fn sum(&mut self) -> LineResult<Expr> {
        let mut left = self.unary()?;
        loop {
            match self.tokens.get(self.position).map(|token| &token.kind) {
                Some(TokenKind::Plus) => {
                    self.position += 1;
                    left = Expr::Add(Box::new(left), Box::new(self.unary()?));
                }
                Some(TokenKind::Minus) => {
                    self.position += 1;
                    left = Expr::Sub(Box::new(left), Box::new(self.unary()?));
                }
                _ => return Ok(left),
            }
        }
    }

    fn unary(&mut self) -> LineResult<Expr> {
        let end = self.end_span();
        let Some(token) = self.next().cloned() else {
            return Err(LineError::new(end, "falta un valor"));
        };
        match token.kind {
            TokenKind::Minus => Ok(Expr::Negate(Box::new(self.unary()?))),
            TokenKind::Plus => self.unary(),
            TokenKind::Int(value) => Ok(Expr::Number(value)),
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
                let inner = self.sum()?;
                let end = self.end_span();
                match self.next().map(|t| t.kind.clone()) {
                    Some(TokenKind::RParen) => Ok(Expr::Modified(modifier, Box::new(inner))),
                    _ => Err(LineError::new(end, "falta el paréntesis de cierre")),
                }
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
        parse_line(text).unwrap().statement.unwrap()
    }

    fn reg(name: &str) -> Reg {
        Reg::parse(name).unwrap()
    }

    #[test]
    fn parses_labels_and_empty_lines() {
        let line = parse_line("a: b: # nada").unwrap();
        assert_eq!(
            line.labels
                .iter()
                .map(|l| l.name.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(line.statement, None);
        assert_eq!(parse_line("   ").unwrap(), Line::default());
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
    }

    #[test]
    fn statement_span_covers_operands() {
        let stmt = statement("  add a0, a1, a2");
        assert_eq!(stmt.span(), Span::new(2, 14));
    }
}
