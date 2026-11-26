//! Lexer de una línea de ensamblador.

use crate::error::{LineError, LineResult, Span};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// Nombre: mnemónico, directiva, registro o símbolo.
    Ident(String),
    Int(i64),
    /// Definición de etiqueta local numérica: `1:` (los dos puntos van incluidos).
    LocalDef(u32),
    /// Referencia a una etiqueta local: `1f` (la siguiente) o `1b` (la anterior).
    LocalRef {
        number: u32,
        forward: bool,
    },
    Str(String),
    /// Modificador de reubicación como `%hi` o `%lo` (sin el `%`).
    Modifier(String),
    Comma,
    LParen,
    RParen,
    Colon,
    Plus,
    Minus,
    Star,
    Slash,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Shl,
    Shr,
    /// Separa varias sentencias en una misma línea.
    Semicolon,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || matches!(c, '_' | '.' | '$')
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$')
}

struct Cursor<'a> {
    chars: &'a [char],
    position: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.position).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.position + offset).copied()
    }

    fn take_while(&mut self, predicate: impl Fn(char) -> bool) -> String {
        let start = self.position;
        while self.peek().is_some_and(&predicate) {
            self.position += 1;
        }
        self.chars[start..self.position].iter().collect()
    }
}

/// Parte una línea en tokens. Lo que hay tras `#` es comentario.
pub fn tokenize(line: &str) -> LineResult<Vec<Token>> {
    let chars: Vec<char> = line.chars().collect();
    let mut cursor = Cursor {
        chars: &chars,
        position: 0,
    };
    let mut tokens = Vec::new();
    while let Some(c) = cursor.peek() {
        let start = cursor.position;
        let kind = match c {
            '#' => break,
            c if c.is_whitespace() => {
                cursor.position += 1;
                continue;
            }
            ',' | '(' | ')' | ':' | '+' | '-' | '*' | '/' | '&' | '|' | '^' | '~' | ';' => {
                cursor.position += 1;
                match c {
                    ',' => TokenKind::Comma,
                    '(' => TokenKind::LParen,
                    ')' => TokenKind::RParen,
                    ':' => TokenKind::Colon,
                    '+' => TokenKind::Plus,
                    '-' => TokenKind::Minus,
                    '*' => TokenKind::Star,
                    '/' => TokenKind::Slash,
                    '&' => TokenKind::Amp,
                    '|' => TokenKind::Pipe,
                    '^' => TokenKind::Caret,
                    '~' => TokenKind::Tilde,
                    _ => TokenKind::Semicolon,
                }
            }
            '<' | '>' => {
                if cursor.peek_at(1) != Some(c) {
                    return Err(LineError::new(
                        Span::new(start, 1),
                        format!("carácter inesperado «{c}» (los desplazamientos son «<<» y «>>»)"),
                    ));
                }
                cursor.position += 2;
                if c == '<' {
                    TokenKind::Shl
                } else {
                    TokenKind::Shr
                }
            }
            '%' => {
                cursor.position += 1;
                let name = cursor.take_while(is_ident_char);
                if name.is_empty() {
                    return Err(LineError::new(
                        Span::new(start, 1),
                        "falta el nombre del modificador tras «%»",
                    ));
                }
                TokenKind::Modifier(name.to_ascii_lowercase())
            }
            '"' => TokenKind::Str(lex_string(&mut cursor)?),
            '\'' => TokenKind::Int(lex_char(&mut cursor)?),
            c if c.is_ascii_digit() => lex_number(&mut cursor)?,
            c if is_ident_start(c) => TokenKind::Ident(cursor.take_while(is_ident_char)),
            other => {
                return Err(LineError::new(
                    Span::new(start, 1),
                    format!("carácter inesperado «{other}»"),
                ));
            }
        };
        tokens.push(Token {
            kind,
            span: Span::new(start, cursor.position - start),
        });
    }
    Ok(tokens)
}

/// Número, o etiqueta local si son cifras decimales seguidas de `:`, `f` o `b` (como en gas:
/// `0b` a secas es la etiqueta local 0 hacia atrás, no un binario vacío).
fn lex_number(cursor: &mut Cursor<'_>) -> LineResult<TokenKind> {
    let start = cursor.position;
    let text = cursor.take_while(|c| c.is_ascii_alphanumeric() || c == '_');
    let span = Span::new(start, text.len());
    let local = |digits: &str| {
        digits.parse::<u32>().map_err(|_| {
            LineError::new(
                span,
                format!("número de etiqueta local demasiado grande «{text}»"),
            )
        })
    };
    if text.bytes().all(|b| b.is_ascii_digit()) && cursor.peek() == Some(':') {
        cursor.position += 1;
        return Ok(TokenKind::LocalDef(local(&text)?));
    }
    if let Some(digits) = text.strip_suffix(['f', 'b'])
        && !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
    {
        return Ok(TokenKind::LocalRef {
            number: local(digits)?,
            forward: text.ends_with('f'),
        });
    }
    let lower = text.to_ascii_lowercase();
    let (digits, radix) = if let Some(rest) = lower.strip_prefix("0x") {
        (rest, 16)
    } else if let Some(rest) = lower.strip_prefix("0b") {
        (rest, 2)
    } else if let Some(rest) = lower.strip_prefix("0o") {
        (rest, 8)
    } else {
        (lower.as_str(), 10)
    };
    let digits: String = digits.chars().filter(|c| *c != '_').collect();
    if digits.is_empty() {
        return Err(LineError::new(span, format!("número incompleto «{text}»")));
    }
    let value = u64::from_str_radix(&digits, radix).map_err(|_| {
        LineError::new(
            span,
            format!("número no válido o de más de 64 bits «{text}»"),
        )
    })?;
    // Las expresiones se calculan en 64 bits con complemento a dos, como en gas: así
    // `0xffffffff80000000 & 0xffffffff` funciona. Si el resultado no cabe en 32 bits lo dice
    // la instrucción o directiva que lo usa.
    Ok(TokenKind::Int(value as i64))
}

fn lex_escape(cursor: &mut Cursor<'_>, start: usize) -> LineResult<char> {
    let escape_start = cursor.position;
    cursor.position += 1;
    let Some(code) = cursor.peek() else {
        return Err(LineError::new(
            Span::new(start, cursor.position - start),
            "secuencia de escape sin terminar",
        ));
    };
    cursor.position += 1;
    let decoded = match code {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '0' => '\0',
        '\\' => '\\',
        '"' => '"',
        '\'' => '\'',
        'x' => {
            let hex: String = (0..2)
                .filter_map(|offset| cursor.peek_at(offset))
                .filter(char::is_ascii_hexdigit)
                .collect();
            if hex.len() != 2 {
                return Err(LineError::new(
                    Span::new(escape_start, 2),
                    "«\\x» necesita dos cifras hexadecimales",
                ));
            }
            cursor.position += 2;
            let value = u8::from_str_radix(&hex, 16).map_err(|_| {
                LineError::new(Span::new(escape_start, 4), "escape hexadecimal no válido")
            })?;
            // Por encima de 0x7F el carácter ocuparía dos bytes en UTF-8 y el escape mentiría.
            if !value.is_ascii() {
                return Err(LineError::new(
                    Span::new(escape_start, 4),
                    "«\\x» solo admite valores ASCII (00 a 7F)",
                ));
            }
            char::from(value)
        }
        other => {
            return Err(LineError::new(
                Span::new(escape_start, 2),
                format!("secuencia de escape desconocida «\\{other}»"),
            ));
        }
    };
    Ok(decoded)
}

fn lex_string(cursor: &mut Cursor<'_>) -> LineResult<String> {
    let start = cursor.position;
    cursor.position += 1;
    let mut value = String::new();
    loop {
        match cursor.peek() {
            None => {
                return Err(LineError::new(
                    Span::new(start, cursor.position - start),
                    "cadena sin cerrar",
                ));
            }
            Some('"') => {
                cursor.position += 1;
                return Ok(value);
            }
            Some('\\') => value.push(lex_escape(cursor, start)?),
            Some(c) => {
                value.push(c);
                cursor.position += 1;
            }
        }
    }
}

fn lex_char(cursor: &mut Cursor<'_>) -> LineResult<i64> {
    let start = cursor.position;
    cursor.position += 1;
    let value = match cursor.peek() {
        Some('\\') => lex_escape(cursor, start)?,
        Some(c) if c != '\'' => {
            cursor.position += 1;
            c
        }
        _ => return Err(LineError::new(Span::new(start, 1), "carácter vacío")),
    };
    if cursor.peek() != Some('\'') {
        return Err(LineError::new(
            Span::new(start, cursor.position - start),
            "falta la comilla de cierre del carácter",
        ));
    }
    cursor.position += 1;
    Ok(i64::from(u32::from(value)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(line: &str) -> Vec<TokenKind> {
        tokenize(line)
            .unwrap()
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn tokenizes_instruction_with_memory_operand_and_comment() {
        assert_eq!(
            kinds("loop: lw a0, -8(sp) # carga"),
            vec![
                TokenKind::Ident("loop".into()),
                TokenKind::Colon,
                TokenKind::Ident("lw".into()),
                TokenKind::Ident("a0".into()),
                TokenKind::Comma,
                TokenKind::Minus,
                TokenKind::Int(8),
                TokenKind::LParen,
                TokenKind::Ident("sp".into()),
                TokenKind::RParen,
            ]
        );
    }

    #[test]
    fn numbers_in_every_base_and_chars() {
        assert_eq!(
            kinds("0x1F 0b101 0o17 42 'a' '\\n'"),
            vec![
                TokenKind::Int(31),
                TokenKind::Int(5),
                TokenKind::Int(15),
                TokenKind::Int(42),
                TokenKind::Int(97),
                TokenKind::Int(10),
            ]
        );
        assert_eq!(kinds("0xFFFFFFFF"), vec![TokenKind::Int(0xffff_ffff)]);
        assert_eq!(
            kinds("0xffffffff80000000"),
            vec![TokenKind::Int(-0x8000_0000)]
        );
    }

    #[test]
    fn strings_with_escapes_and_modifiers() {
        assert_eq!(
            kinds(r#".string "hola\n\x41""#),
            vec![
                TokenKind::Ident(".string".into()),
                TokenKind::Str("hola\nA".into()),
            ]
        );
        assert_eq!(kinds("%HI(x)")[0], TokenKind::Modifier("hi".into()));
    }

    #[test]
    fn reports_errors_with_position() {
        let error = tokenize("li a0, 0x1_0000_0000_0000_0000").unwrap_err();
        assert_eq!(error.span.start, 7);
        assert_eq!(tokenize("a < b").unwrap_err().span, Span::new(2, 1));
        let error = tokenize("  .string \"abc").unwrap_err();
        assert_eq!(error.span.start, 10);
        assert_eq!(
            tokenize("add a0, a1, @").unwrap_err().span,
            Span::new(12, 1)
        );
        assert!(tokenize("0x").is_err());
        assert!(tokenize("12ab").is_err());
        assert!(tokenize("1fb").is_err());
    }

    #[test]
    fn local_labels_operators_and_separators() {
        assert_eq!(
            kinds("1: bne a0, a1, 1b; j 12f"),
            vec![
                TokenKind::LocalDef(1),
                TokenKind::Ident("bne".into()),
                TokenKind::Ident("a0".into()),
                TokenKind::Comma,
                TokenKind::Ident("a1".into()),
                TokenKind::Comma,
                TokenKind::LocalRef {
                    number: 1,
                    forward: false
                },
                TokenKind::Semicolon,
                TokenKind::Ident("j".into()),
                TokenKind::LocalRef {
                    number: 12,
                    forward: true
                },
            ]
        );
        assert_eq!(
            kinds("~1 << 2 >> 3 & 4 | 5 ^ 6 * 7 / 8"),
            vec![
                TokenKind::Tilde,
                TokenKind::Int(1),
                TokenKind::Shl,
                TokenKind::Int(2),
                TokenKind::Shr,
                TokenKind::Int(3),
                TokenKind::Amp,
                TokenKind::Int(4),
                TokenKind::Pipe,
                TokenKind::Int(5),
                TokenKind::Caret,
                TokenKind::Int(6),
                TokenKind::Star,
                TokenKind::Int(7),
                TokenKind::Slash,
                TokenKind::Int(8),
            ]
        );
        // «0b» sin cifras es la etiqueta local 0 hacia atrás; «0x1f» sigue siendo un número.
        assert_eq!(
            kinds("0b 0x1f"),
            vec![
                TokenKind::LocalRef {
                    number: 0,
                    forward: false
                },
                TokenKind::Int(0x1f)
            ]
        );
        assert_eq!(kinds(r#".string "a;b""#)[1], TokenKind::Str("a;b".into()));
    }
}
