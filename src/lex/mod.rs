mod byte_search;
mod tok;

use crate::err::PgError;
use byte_search::find_byte;

pub use tok::{Token, Type};

const IDENT_CONT: u8 = 1 << 0;
const IDENT_START: u8 = 1 << 1;
const DIGIT: u8 = 1 << 2;
const NUM_CONT: u8 = 1 << 3;
const WS: u8 = 1 << 4;

const fn build_class_table() -> [u8; 256] {
    let mut t = [0u8; 256];
    let mut c: usize = 0;
    while c < 256 {
        let b = c as u8;
        let mut v: u8 = 0;
        let alpha = (b >= b'a' && b <= b'z') || (b >= b'A' && b <= b'Z');
        let digit = b >= b'0' && b <= b'9';
        if alpha || digit || b == b'_' {
            v |= IDENT_CONT;
        }
        if alpha || b == b'_' {
            v |= IDENT_START;
        }
        if digit {
            v |= DIGIT;
        }
        if digit || b == b'.' {
            v |= NUM_CONT;
        }
        if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
            v |= WS;
        }
        t[c] = v;
        c += 1;
    }
    t
}

const CLASS: [u8; 256] = build_class_table();

#[inline(always)]
fn class_of(b: u8) -> u8 {
    CLASS[b as usize]
}

#[derive(Debug)]
pub struct Lexer<'l> {
    input: &'l [u8],
    pos: usize,
}

#[inline]
fn as_keyword_type(inner: &str) -> Option<Type<'_>> {
    Some(match inner {
        "import" => Type::Import,
        "as" => Type::As,
        "true" => Type::True,
        "false" => Type::False,
        "let" => Type::Let,
        "fn" => Type::Fn,
        "match" => Type::Match,
        "str" => Type::Str,
        "int" => Type::Int,
        "double" => Type::Double,
        "bool" => Type::Bool,
        "void" => Type::Void,
        _ => return None,
    })
}

impl<'l> Lexer<'l> {
    pub fn new(input: &'l [u8]) -> Self {
        Self { input, pos: 0 }
    }

    #[inline]
    fn make_tok(start: usize, t: Type<'l>) -> Token<'l> {
        Token { start, t }
    }

    fn make_err(&self, msg: impl Into<String>, start: usize) -> PgError {
        PgError {
            msg: msg.into(),
            start,
            len: self.pos.saturating_sub(start),
        }
    }

    #[inline]
    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos + 1).copied()
    }

    /// skip whitespace and comments
    fn skip_whitespace(&mut self) {
        let bytes = self.input;
        let mut p = self.pos;
        loop {
            while p < bytes.len() && class_of(bytes[p]) & WS != 0 {
                p += 1;
            }
            if p < bytes.len() && bytes[p] == b'#' {
                let end = find_byte(b'\n', &bytes[p..]).map_or(bytes.len(), |i| p + i);
                p = end;
                continue;
            }
            break;
        }
        self.pos = p;
    }

    pub fn one(&mut self) -> Result<Token<'l>, PgError> {
        self.skip_whitespace();

        let start = self.pos;

        if start >= self.input.len() {
            return Ok(Self::make_tok(start, Type::Eof));
        }

        let t = match self.input[start] {
            b'(' => Self::make_tok(start, Type::BraceLeft),
            b')' => Self::make_tok(start, Type::BraceRight),
            b'+' => Self::make_tok(start, Type::Plus),
            b'-' => Self::make_tok(start, Type::Minus),
            b'*' => Self::make_tok(start, Type::Asteriks),
            b'/' => Self::make_tok(start, Type::Slash),
            b'=' if matches!(self.peek(), Some(b'=')) => {
                self.pos += 1;
                Self::make_tok(start, Type::DoubleEqual)
            }
            b'=' => Self::make_tok(start, Type::Equal),
            b'<' => Self::make_tok(start, Type::LessThan),
            b'>' => Self::make_tok(start, Type::GreaterThan),
            b'!' if matches!(self.peek(), Some(b'=')) => {
                self.pos += 1;
                Self::make_tok(start, Type::NotEqual)
            }
            b'!' => Self::make_tok(start, Type::Exclaim),
            b'?' => Self::make_tok(start, Type::Question),
            b'.' => Self::make_tok(start, Type::Dot),
            b':' => Self::make_tok(start, Type::Colon),
            b'[' => Self::make_tok(start, Type::BraketLeft),
            b']' => Self::make_tok(start, Type::BraketRight),
            b'{' => Self::make_tok(start, Type::CurlyLeft),
            b'}' => Self::make_tok(start, Type::CurlyRight),
            b'"' => {
                self.pos += 1;
                let body_start = self.pos;
                let bytes = self.input;

                let end = match find_byte(b'"', &bytes[body_start..]) {
                    Some(i) => body_start + i,
                    None => {
                        self.pos = bytes.len();
                        return Err(self.make_err("Unterminated string", start));
                    }
                };

                self.pos = end;

                Self::make_tok(
                    start,
                    Type::S(
                        str::from_utf8(&bytes[body_start..end])
                            .map_err(|_| self.make_err("Invalid ut8 input", start))?,
                    ),
                )
            }
            c if class_of(c) & IDENT_START != 0 => {
                let bytes = self.input;
                let mut p = start + 1;
                while p < bytes.len() && class_of(bytes[p]) & IDENT_CONT != 0 {
                    p += 1;
                }
                self.pos = p;

                // SAFETY: only bytes accepted by IDENT_START/IDENT_CONT are
                // included, all of which are ASCII (< 128) and therefore valid
                // UTF-8 on their own.
                let inner = unsafe { str::from_utf8_unchecked(&bytes[start..p]) };

                let t = as_keyword_type(inner).unwrap_or(Type::Ident(inner));
                return Ok(Self::make_tok(start, t));
            }
            c if class_of(c) & DIGIT != 0 => {
                let bytes = self.input;
                let mut p = start + 1;
                let mut is_double = false;
                while p < bytes.len() {
                    let b = bytes[p];
                    if class_of(b) & NUM_CONT == 0 {
                        break;
                    }
                    is_double |= b == b'.';
                    p += 1;
                }
                self.pos = p;

                // SAFETY: only ASCII digits and '.' are accepted, valid UTF-8.
                let inner = unsafe { str::from_utf8_unchecked(&bytes[start..p]) };

                return Ok(Self::make_tok(
                    start,
                    if is_double {
                        Type::D(inner)
                    } else {
                        Type::I(inner)
                    },
                ));
            }
            c => {
                return Err(self.make_err(format!("Unknown character `{}`", c as char), start));
            }
        };

        self.pos += 1;
        Ok(t)
    }

    #[cfg(test)]
    pub fn all(&mut self) -> Result<Vec<Token<'l>>, PgError> {
        let mut raindrain = Vec::with_capacity(1024);
        loop {
            let t = self.one()?;
            if t.t == Type::Eof {
                break;
            } else {
                raindrain.push(t);
            }
        }
        Ok(raindrain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(input: &str) -> Vec<Type<'_>> {
        let mut l = Lexer::new(input.as_bytes());
        l.all()
            .expect("lexer error")
            .into_iter()
            .map(|t| t.t)
            .collect()
    }

    #[test]
    fn single_char_tokens() {
        let toks = lex("()+-*/=<>![]{}.:?");
        assert_eq!(
            toks,
            vec![
                Type::BraceLeft,
                Type::BraceRight,
                Type::Plus,
                Type::Minus,
                Type::Asteriks,
                Type::Slash,
                Type::Equal,
                Type::LessThan,
                Type::GreaterThan,
                Type::Exclaim,
                Type::BraketLeft,
                Type::BraketRight,
                Type::CurlyLeft,
                Type::CurlyRight,
                Type::Dot,
                Type::Colon,
                Type::Question,
            ]
        );
    }

    #[test]
    fn double_char_tokens() {
        let toks = lex("== !=");
        assert_eq!(toks, vec![Type::DoubleEqual, Type::NotEqual]);
    }

    #[test]
    fn identifiers() {
        let toks = lex("foo bar baz");
        assert_eq!(
            toks,
            vec![Type::Ident("foo"), Type::Ident("bar"), Type::Ident("baz"),]
        );
    }

    #[test]
    fn integers() {
        let toks = lex("0 123 42");
        assert_eq!(toks, vec![Type::I("0"), Type::I("123"), Type::I("42"),]);
    }

    #[test]
    fn doubles() {
        let toks = lex("1.0 3.14");
        assert_eq!(toks, vec![Type::D("1.0"), Type::D("3.14"),]);
    }

    #[test]
    fn mixed_expression() {
        let toks = lex("(sum 1 2)");
        assert_eq!(
            toks,
            vec![
                Type::BraceLeft,
                Type::Ident("sum"),
                Type::I("1"),
                Type::I("2"),
                Type::BraceRight,
            ]
        );
    }

    #[test]
    fn whitespace_and_newlines() {
        let toks = lex("(\n  foo\t42 \r)");
        assert_eq!(
            toks,
            vec![
                Type::BraceLeft,
                Type::Ident("foo"),
                Type::I("42"),
                Type::BraceRight,
            ]
        );
    }

    #[test]
    fn string_literal() {
        let toks = lex("\"hello\"");
        assert_eq!(toks, vec![Type::S("hello")]);
    }

    #[test]
    #[should_panic]
    fn unknown_character_errors() {
        let mut l = Lexer::new(b"$");
        l.one().unwrap();
    }

    #[test]
    fn keywords() {
        assert_eq!(
            lex("
    import
    true
    false
    let
    fn
    match
    str
    int
    double
    bool
    void
    "),
            vec![
                Type::Import,
                Type::True,
                Type::False,
                Type::Let,
                Type::Fn,
                Type::Match,
                Type::Str,
                Type::Int,
                Type::Double,
                Type::Bool,
                Type::Void,
            ]
        )
    }

    #[test]
    #[should_panic]
    fn unterminated_string() {
        let mut l = Lexer::new(b"\"hello");
        l.one().unwrap();
    }

    #[test]
    fn leading_dot_numbers() {
        let toks = lex("0.5 1.");
        assert_eq!(toks, vec![Type::D("0.5"), Type::D("1.")]);
    }

    #[test]
    fn multiple_dots_in_number() {
        let toks = lex("1.2.3");
        assert_eq!(toks, vec![Type::D("1.2.3")]);
    }

    #[test]
    fn identifier_keyword_adjacency() {
        let toks = lex("truex fnx");
        assert_eq!(toks, vec![Type::Ident("truex"), Type::Ident("fnx"),]);
    }

    #[test]
    fn repeated_eof_calls() {
        let mut l = Lexer::new(b"");
        let t1 = l.one().unwrap();
        let t2 = l.one().unwrap();
        assert_eq!(t1.t, Type::Eof);
        assert_eq!(t2.t, Type::Eof);
    }

    #[test]
    fn unknown_character_error_carries_position() {
        let mut l = Lexer::new(b"  $");
        let err = l.one().expect_err("expected lexer error on `$`");
        assert_eq!(err.start, 2, "byte offset of the offending character");
    }

    #[test]
    fn error_after_newline_does_not_underflow() {
        let mut l = Lexer::new(b"\n$");
        let err = l.one().expect_err("expected lexer error on `$`");
        assert_eq!(err.start, 1, "byte offset of the offending character");
    }
}
