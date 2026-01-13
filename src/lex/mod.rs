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

    pub fn next(&mut self) -> Result<Token<'l>, PgError> {
        while matches!(self.cur(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.advance()
        }

        let t = match self
            .cur()
            .ok_or_else(|| self.make_err("Unexpected end of file", self.col))?
        {
            b'(' => self.make_tok(Type::DelimitLeft),
            b')' => self.make_tok(Type::DelimitRight),
            b'+' => self.make_tok(Type::Plus),
            b'-' => self.make_tok(Type::Minus),
            b'*' => self.make_tok(Type::Asteriks),
            b'/' => self.make_tok(Type::Slash),
            b'=' => self.make_tok(Type::Equal),
            b'<' => self.make_tok(Type::LessThan),
            b'>' => self.make_tok(Type::GreaterThan),
            b'!' => self.make_tok(Type::Exlaim),
            b':' if matches!(self.peek(), Some(b':')) => {
                self.advance();
                self.make_tok(Type::DoubleColon)
            }
            b':' => self.make_tok(Type::Colon),
            b'[' => self.make_tok(Type::BraketLeft),
            b']' => self.make_tok(Type::BraketRight),
            b'{' => self.make_tok(Type::CurlyLeft),
            b'}' => self.make_tok(Type::CurlyRight),
            b'"' => todo!("strings"),
            c => {
                if c.is_ascii_alphabetic() {
                    let start = self.pos;
                    self.advance();
                    while self.cur().is_some_and(|b| b.is_ascii_alphabetic()) {
                        self.advance();
                    }

                    self.make_tok(Type::String(
                        str::from_utf8(&self.input[start..self.pos])
                            .map_err(|_| self.make_err("Invalid ut8 input", self.col))?,
                    ))
                } else if c.is_ascii_digit() {
                    todo!("numbers");
                } else {
                    return Err(self.make_err(format!("Unknown charcter `{}`", c), self.col));
                }
            }
        };

        self.advance();

        Ok(t)
    }

    pub fn all(&mut self) -> Result<Vec<Token<'l>>, PgError> {
        let mut raindrain = Vec::with_capacity(1024);
        while !self.at_end() {
            raindrain.push(self.next()?);
        }
        Ok(raindrain)
    }
}
