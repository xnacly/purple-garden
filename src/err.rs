use crate::{
    ast::Node,
    lex::{Token, Type},
};

#[derive(Debug)]
pub struct PgError {
    msg: Option<String>,
    line: usize,
    start: usize,
    end: usize,
}

impl From<&Token<'_>> for PgError {
    fn from(value: &Token) -> Self {
        let len = match value.t {
            Type::String(i) | Type::Ident(i) | Type::Double(i) | Type::Integer(i) => i.len(),
            Type::True => 4,
            Type::False | Type::Match => 5,
            Type::Let | Type::Std | Type::For => 3,
            Type::Fn | Type::DoubleColon => 2,
            // all others are a single byte long
            _ => 1,
        };
        PgError {
            msg: None,
            line: value.line,
            start: value.col,
            end: value.col + len,
        }
    }
}

impl From<&Node<'_>> for PgError {
    fn from(value: &Node<'_>) -> Self {
        (&value.token).into()
    }
}

impl PgError {
    // TODO: replace with writing to some kind of std::writer
    pub fn render(self) {
        println!(
            "err: {} at l:{}:{}-{}",
            self.msg.unwrap_or_default(),
            self.line,
            self.start,
            self.end
        );
    }

    pub fn with_msg(msg: impl Into<String>, from: impl Into<PgError>) -> Self {
        let mut conv = from.into();
        conv.msg = Some(msg.into());
        conv
    }
}
