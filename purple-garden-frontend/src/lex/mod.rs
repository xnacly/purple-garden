mod byte_search;
mod tok;

use crate::diagnostic::{Diagnostic, Span};
use byte_search::{find_byte, skip_ident_cont, skip_num_cont};

pub use tok::{KEYWORD_DOCS, KeywordDoc, TYPE_DOCS, Token, Type, TypeDoc, keyword_doc, type_doc};

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
    pub(crate) diagnostics: Vec<Diagnostic>,
}

#[inline]
fn as_keyword_type(inner: &str) -> Option<Type<'_>> {
    Type::from_keyword(inner)
}

impl<'l> Lexer<'l> {
    #[must_use]
    pub fn new(input: &'l [u8]) -> Self {
        Self {
            input,
            pos: 0,
            diagnostics: Vec::new(),
        }
    }

    #[inline]
    fn make_tok(start: usize, t: Type<'l>) -> Token<'l> {
        Token { start, t }
    }

    fn make_err(&self, msg: impl Into<String>, start: usize) -> Diagnostic {
        Diagnostic::new(msg, Span::new(start, self.pos.saturating_sub(start)))
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
            if p + 1 < bytes.len() && bytes[p] == b'#' && bytes[p + 1] == b'!' {
                break;
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

    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    pub fn one(&mut self) -> Token<'l> {
        loop {
            self.skip_whitespace();

            let start = self.pos;

            if start >= self.input.len() {
                return Self::make_tok(start, Type::Eof);
            }

            let t = match self.input[start] {
                b'#' if matches!(self.peek(), Some(b'!')) => {
                    let body_start = start + 2;
                    let end = find_byte(b'\n', &self.input[body_start..])
                        .map_or(self.input.len(), |i| body_start + i);
                    self.pos = end.saturating_sub(1);

                    let body = &self.input[body_start..end];
                    let body = body.strip_prefix(b" ").unwrap_or(body);
                    let Ok(doc) = str::from_utf8(body) else {
                        self.diagnostics
                            .push(self.make_err("Invalid utf8 input", start));
                        continue;
                    };

                    Self::make_tok(start, Type::Doc(doc))
                }
                b'(' => Self::make_tok(start, Type::BraceLeft),
                b')' => Self::make_tok(start, Type::BraceRight),
                b'+' => Self::make_tok(start, Type::Plus),
                b'-' => Self::make_tok(start, Type::Minus),
                b'*' => Self::make_tok(start, Type::Asteriks),
                b'/' => Self::make_tok(start, Type::Slash),
                b'%' => Self::make_tok(start, Type::Percent),
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

                    let end = if let Some(i) = find_byte(b'"', &bytes[body_start..]) {
                        body_start + i
                    } else {
                        let line_end = find_byte(b'\n', &bytes[body_start..])
                            .map_or(bytes.len(), |i| body_start + i);
                        self.pos = line_end;
                        self.diagnostics
                            .push(self.make_err("Unterminated string", start));
                        continue;
                    };

                    self.pos = end;

                    let Ok(inner) = str::from_utf8(&bytes[body_start..end]) else {
                        self.diagnostics
                            .push(self.make_err("Invalid utf8 input", start));
                        self.pos += 1;
                        continue;
                    };

                    Self::make_tok(start, Type::S(inner))
                }
                c if class_of(c) & IDENT_START != 0 => {
                    let bytes = self.input;
                    let p = start + 1 + skip_ident_cont(&self.input[start + 1..]);
                    self.pos = p;

                    // SAFETY: only bytes accepted by IDENT_START/IDENT_CONT are
                    // included, all of which are ASCII (< 128) and therefore valid
                    // UTF-8 on their own.
                    let inner = unsafe { str::from_utf8_unchecked(&bytes[start..p]) };

                    let t = as_keyword_type(inner).unwrap_or(Type::Ident(inner));
                    return Self::make_tok(start, t);
                }
                c if class_of(c) & DIGIT != 0 => {
                    let bytes = self.input;
                    let p = start + 1 + skip_num_cont(&self.input[start + 1..]);
                    self.pos = p;

                    let is_double = if let Some(dot) = find_byte(b'.', &bytes[start + 1..p]) {
                        if find_byte(b'.', &bytes[start + 1 + dot + 1..p]).is_some() {
                            self.diagnostics
                                .push(self.make_err("Invalid numeric literal", start));
                            continue;
                        }
                        true
                    } else {
                        false
                    };

                    // SAFETY: only ASCII digits and '.' are accepted, valid UTF-8.
                    let inner = unsafe { str::from_utf8_unchecked(&bytes[start..p]) };

                    return Self::make_tok(
                        start,
                        if is_double {
                            Type::D(inner)
                        } else {
                            Type::I(inner)
                        },
                    );
                }
                c => {
                    self.pos += 1;
                    self.diagnostics
                        .push(self.make_err(format!("Unknown character `{}`", c as char), start));
                    continue;
                }
            };

            self.pos += 1;
            return t;
        }
    }

    #[cfg(test)]
    pub fn all(&mut self) -> Vec<Token<'l>> {
        let mut raindrain = Vec::with_capacity(1024);
        loop {
            let t = self.one();
            if t.t == Type::Eof {
                break;
            }
            raindrain.push(t);
        }
        raindrain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(input: &str) -> Vec<Type<'_>> {
        let mut l = Lexer::new(input.as_bytes());
        l.all().into_iter().map(|t| t.t).collect()
    }

    #[test]
    fn single_char_tokens() {
        let toks = lex("()+-*/%=<>![]{}.:?");
        assert_eq!(
            toks,
            vec![
                Type::BraceLeft,
                Type::BraceRight,
                Type::Plus,
                Type::Minus,
                Type::Asteriks,
                Type::Slash,
                Type::Percent,
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
    fn comments_are_skipped() {
        let toks = lex("1 # ignore me\n2");
        assert_eq!(toks, vec![Type::I("1"), Type::I("2")]);
    }

    #[test]
    fn string_literal() {
        let toks = lex("\"hello\"");
        assert_eq!(toks, vec![Type::S("hello")]);
    }

    #[test]
    fn unknown_character_errors() {
        let mut l = Lexer::new(b"$");
        assert_eq!(l.one().t, Type::Eof);
        assert_eq!(l.diagnostics().len(), 1);
    }

    #[test]
    fn unknown_character_recovers_to_next_token() {
        let mut l = Lexer::new(b"$ let x = 1");
        assert_eq!(l.one().t, Type::Let);
        assert_eq!(l.one().t, Type::Ident("x"));
        assert_eq!(l.diagnostics().len(), 1);
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
    Str
    Int
    Double
    Bool
    Void
    Option
    Array
    Foreign
    Record
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
                Type::Option,
                Type::Array,
                Type::Foreign,
                Type::Record,
            ]
        );
    }

    #[test]
    fn unterminated_string() {
        let mut l = Lexer::new(b"\"hello");
        assert_eq!(l.one().t, Type::Eof);
        assert_eq!(l.diagnostics().len(), 1);
    }

    #[test]
    fn unterminated_string_recovers_at_newline() {
        let mut l = Lexer::new(b"\"hello\nlet x = 1");
        assert_eq!(l.one().t, Type::Let);
        assert_eq!(l.one().t, Type::Ident("x"));
        let err = &l.diagnostics()[0];
        assert_eq!(err.primary.span.start, 0);
        assert_eq!(err.primary.span.len, 6);
    }

    #[test]
    fn invalid_utf8_string_reports_and_continues() {
        let mut l = Lexer::new(b"\"\xff\" let");
        assert_eq!(l.one().t, Type::Let);
        assert_eq!(l.diagnostics().len(), 1);
        assert_eq!(l.diagnostics()[0].primary.span.start, 0);
    }

    #[test]
    fn leading_dot_numbers() {
        let toks = lex("0.5 1.");
        assert_eq!(toks, vec![Type::D("0.5"), Type::D("1.")]);
    }

    #[test]
    fn multiple_dots_in_number() {
        let mut l = Lexer::new(b"1.2.3 4");
        assert_eq!(l.one().t, Type::I("4"));
        assert_eq!(l.diagnostics().len(), 1);
    }

    #[test]
    fn identifier_keyword_adjacency() {
        let toks = lex("truex fnx");
        assert_eq!(toks, vec![Type::Ident("truex"), Type::Ident("fnx"),]);
    }

    #[test]
    fn repeated_eof_calls() {
        let mut l = Lexer::new(b"");
        let t1 = l.one();
        let t2 = l.one();
        assert_eq!(t1.t, Type::Eof);
        assert_eq!(t2.t, Type::Eof);
    }

    #[test]
    fn unknown_character_error_carries_position() {
        let mut l = Lexer::new(b"  $");
        assert_eq!(l.one().t, Type::Eof);
        let err = &l.diagnostics()[0];
        assert_eq!(
            err.primary.span.start, 2,
            "byte offset of the offending character"
        );
    }

    #[test]
    fn error_after_newline_does_not_underflow() {
        let mut l = Lexer::new(b"\n$");
        assert_eq!(l.one().t, Type::Eof);
        let err = &l.diagnostics()[0];
        assert_eq!(
            err.primary.span.start, 1,
            "byte offset of the offending character"
        );
    }
}
