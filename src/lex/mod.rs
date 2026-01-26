mod tok;

use crate::err::PgError;

pub use tok::{Token, Type};

#[derive(Debug)]
pub struct Lexer<'l> {
    input: &'l [u8],
    pos: usize,
    line: usize,
    col: usize,
}

impl<'l> Lexer<'l> {
    pub fn new(input: &'l [u8]) -> Self {
        Self {
            input,
            pos: 0,
            line: 0,
            col: 0,
        }
    }

    fn make_tok(&self, t: Type<'l>) -> Token<'l> {
        Token {
            line: self.line,
            col: self.col,
            t,
        }
    }

    fn make_err(&self, msg: impl Into<String>, start: usize) -> PgError {
        PgError {
            msg: Some(msg.into()),
            line: self.line,
            start,
            end: self.col,
        }
    }

    fn advance(&mut self) {
        if let Some(b) = self.cur() {
            self.pos += 1;
            if b == b'\n' {
                self.line += 1;
                self.col = 0;
            } else {
                self.col += 1;
            }
        }
    }

    fn cur(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos + 1).copied()
    }

    fn at_end(&mut self) -> bool {
        self.pos >= self.input.len()
    }

    fn as_keyword(&self, inner: &'l str) -> Option<Token<'l>> {
        let as_type = Some(match inner {
            "true" => Type::True,
            "false" => Type::False,
            "let" => Type::Let,
            "fn" => Type::Fn,
            "match" => Type::Match,
            "for" => Type::For,
            "str" => Type::TStr,
            "int" => Type::TInt,
            "double" => Type::TDouble,
            "bool" => Type::TBool,
            "void" => Type::TVoid,
            _ => return None,
        })?;

        Some(self.make_tok(as_type))
    }

    pub fn next(&mut self) -> Result<Token<'l>, PgError> {
        if self.cur().is_some_and(|c| c == b'#') {
            self.advance();
            while !self.at_end() && self.cur().is_some_and(|c| c != b'\n') {
                self.advance();
            }
        }

        while matches!(self.cur(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.advance()
        }

        if self.at_end() {
            return Ok(self.make_tok(Type::Eof));
        }

        let t = match self
            .cur()
            .ok_or_else(|| self.make_err("Unexpected end of file", self.col))?
        {
            b'(' => self.make_tok(Type::BraceLeft),
            b')' => self.make_tok(Type::BraceRight),
            b'+' => self.make_tok(Type::Plus),
            b'-' => self.make_tok(Type::Minus),
            b'*' => self.make_tok(Type::Asteriks),
            b'/' => self.make_tok(Type::Slash),
            b'=' if matches!(self.peek(), Some(b'=')) => {
                self.advance();
                self.make_tok(Type::DoubleEqual)
            }
            b'=' => self.make_tok(Type::Equal),
            b'<' => self.make_tok(Type::LessThan),
            b'>' => self.make_tok(Type::GreaterThan),
            b'!' => self.make_tok(Type::Exlaim),
            b'?' => self.make_tok(Type::Question),
            b':' if matches!(self.peek(), Some(b':')) => {
                self.advance();
                self.make_tok(Type::DoubleColon)
            }
            b':' => self.make_tok(Type::Colon),
            b'[' => self.make_tok(Type::BraketLeft),
            b']' => self.make_tok(Type::BraketRight),
            b'{' => self.make_tok(Type::CurlyLeft),
            b'}' => self.make_tok(Type::CurlyRight),
            b'"' => {
                self.advance();
                let start = self.pos;

                while !self.at_end() && !matches!(self.cur(), Some(b'"')) {
                    // TODO: deal with escapes here
                    self.advance();
                }

                if self.cur() != Some(b'"') {
                    return Err(self.make_err("Unterminated string", self.col));
                }

                self.make_tok(Type::String(
                    str::from_utf8(&self.input[start..self.pos])
                        .map_err(|_| self.make_err("Invalid ut8 input", self.col))?,
                ))
            }
            c if c.is_ascii_alphabetic() => {
                let start = self.pos;
                self.advance();
                while self
                    .cur()
                    .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
                {
                    self.advance();
                }

                let inner = str::from_utf8(&self.input[start..self.pos])
                    .map_err(|_| self.make_err("Invalid ut8 input", self.col))?;

                return Ok(match self.as_keyword(inner) {
                    Some(as_keyword) => as_keyword,
                    None => self.make_tok(Type::Ident(inner)),
                });
            }
            c if c.is_ascii_digit() => {
                let start = self.pos;
                let mut is_double = false;
                while self.cur().is_some_and(|b| b.is_ascii_digit() || b == b'.') {
                    is_double = is_double || self.cur() == Some(b'.');
                    self.advance();
                }

                let inner = str::from_utf8(&self.input[start..self.pos])
                    .map_err(|_| self.make_err("Invalid ut8 input", self.col))?;

                return Ok(if is_double {
                    self.make_tok(Type::Double(inner))
                } else {
                    self.make_tok(Type::Integer(inner))
                });
            }
            c => {
                return Err(self.make_err(format!("Unknown character `{}`", c as char), self.col));
            }
        };

        self.advance();

        Ok(t)
    }

    #[cfg(test)]
    pub fn all(&mut self) -> Result<Vec<Token<'l>>, PgError> {
        let mut raindrain = Vec::with_capacity(1024);
        loop {
            let t = self.next()?;
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
        let toks = lex("()+-*/=<>![]{}:?");
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
                Type::Exlaim,
                Type::BraketLeft,
                Type::BraketRight,
                Type::CurlyLeft,
                Type::CurlyRight,
                Type::Colon,
                Type::Question,
            ]
        );
    }

    #[test]
    fn double_char_tokens() {
        let toks = lex(":: ==");
        assert_eq!(toks, vec![Type::DoubleColon, Type::DoubleEqual]);
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
        assert_eq!(
            toks,
            vec![
                Type::Integer("0"),
                Type::Integer("123"),
                Type::Integer("42"),
            ]
        );
    }

    #[test]
    fn doubles() {
        let toks = lex("1.0 3.14");
        assert_eq!(toks, vec![Type::Double("1.0"), Type::Double("3.14"),]);
    }

    #[test]
    fn mixed_expression() {
        let toks = lex("(sum 1 2)");
        assert_eq!(
            toks,
            vec![
                Type::BraceLeft,
                Type::Ident("sum"),
                Type::Integer("1"),
                Type::Integer("2"),
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
                Type::Integer("42"),
                Type::BraceRight,
            ]
        );
    }

    #[test]
    fn string_literal() {
        let toks = lex("\"hello\"");
        assert_eq!(toks, vec![Type::String("hello")]);
    }

    #[test]
    #[should_panic]
    fn unknown_character_errors() {
        let mut l = Lexer::new(b"$");
        l.next().unwrap();
    }

    #[test]
    fn keywords() {
        assert_eq!(
            lex("
    true
    false
    let
    fn
    match
    for
    str
    int
    double
    bool
    void
    "),
            vec![
                Type::True,
                Type::False,
                Type::Let,
                Type::Fn,
                Type::Match,
                Type::For,
                Type::TStr,
                Type::TInt,
                Type::TDouble,
                Type::TBool,
                Type::TVoid,
            ]
        )
    }

    #[test]
    #[should_panic]
    fn unterminated_string() {
        let mut l = Lexer::new(b"\"hello");
        l.next().unwrap();
    }

    #[test]
    fn leading_dot_numbers() {
        let toks = lex("0.5 1.");
        assert_eq!(toks, vec![Type::Double("0.5"), Type::Double("1.")]);
    }

    #[test]
    fn multiple_dots_in_number() {
        let toks = lex("1.2.3");
        assert_eq!(toks, vec![Type::Double("1.2.3")]);
    }

    #[test]
    fn identifier_keyword_adjacency() {
        let toks = lex("truex fnx");
        assert_eq!(toks, vec![Type::Ident("truex"), Type::Ident("fnx"),]);
    }

    #[test]
    fn repeated_eof_calls() {
        let mut l = Lexer::new(b"");
        let t1 = l.next().unwrap();
        let t2 = l.next().unwrap();
        assert_eq!(t1.t, Type::Eof);
        assert_eq!(t2.t, Type::Eof);
    }

    #[test]
    fn weird_colons() {
        let toks = lex("::: :: :");
        assert_eq!(
            toks,
            vec![
                Type::DoubleColon,
                Type::Colon,
                Type::DoubleColon,
                Type::Colon,
            ]
        );
    }
}
