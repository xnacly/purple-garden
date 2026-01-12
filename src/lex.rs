use crate::err::PgError;

#[derive(Debug, Clone)]
pub enum Type<'t> {
    DelimitLeft,
    DelimitRight,
    Plus,
    Minus,
    Asteriks,
    Slash,
    Equal,
    LessThan,
    GreaterThan,
    Exlaim,
    Colon,
    DoubleColon,
    BraketLeft,
    BraketRight,
    CurlyLeft,
    CurlyRight,

    String(&'t str),
    Ident(&'t str),
    Double(&'t str),
    Integer(&'t str),

    True,
    False,
    Let,
    Fn,
    Match,
    For,
}

#[derive(Debug)]
pub struct Token<'t> {
    pub line: usize,
    pub col: usize,
    pub t: Type<'t>,
}

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

    fn advance(&mut self) {
        self.pos += 1;
        self.col += 1;
        if let Some(b'\n') = self.cur() {
            self.line += 1;
            self.col = 0;
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

        let t = match self.cur().ok_or_else(|| PgError {
            msg: Some("Unexpected end of file".into()),
            line: self.line,
            start: self.col,
            end: self.col,
        })? {
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

            // TODO: atoms
            // TODO: keywords
            c @ _ => {
                return Err(PgError {
                    msg: Some(format!("Unknown charcter `{}`", c)),
                    line: self.line,
                    start: self.col,
                    end: self.col,
                });
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
