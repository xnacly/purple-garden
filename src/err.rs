use crate::{
    lex::{Token, Type},
    vm::Anomaly,
};

#[derive(Debug)]
pub struct PgError {
    pub title: &'static str,
    pub msg: Option<String>,
    pub line: usize,
    pub start: usize,
    pub len: usize,
}

impl From<&Token<'_>> for PgError {
    fn from(value: &Token) -> Self {
        let len = match value.t {
            Type::S(i) | Type::Ident(i) | Type::D(i) | Type::I(i) => i.len(),
            Type::True => 4,
            Type::False | Type::Match => 5,
            Type::Let | Type::For => 3,
            Type::Fn | Type::DoubleColon => 2,
            // all others are a single byte long
            _ => 1,
        };
        PgError {
            title: "temp",
            msg: None,
            line: value.line,
            start: value.col,
            len,
        }
    }
}

impl From<Anomaly> for PgError {
    fn from(value: Anomaly) -> Self {
        // TODO: do some prep in anomaly for finding out which ast node resulted in what bytecode
        // ranges
        PgError {
            title: "Virtual Machine Anomaly",
            msg: Some(value.as_str().to_string()),
            line: 0,
            start: 0,
            len: 0,
        }
    }
}

impl PgError {
    pub fn render(self, lines: &[&str]) {
        println!("-> err: {}", self.title);
        println!("   {}\n", self.msg.unwrap_or_default());

        let prev = self.line.saturating_sub(1);
        if let Some(prev_line) = lines.get(prev)
            && prev != self.line
        {
            println!("{:03} | {prev_line}", prev);
        }

        if let Some(line) = lines.get(self.line) {
            println!("{:03} | {line}", self.line);
            println!("{}~ here", " ".repeat(self.start + 7))
        }
    }

    pub fn with_msg(title: &'static str, msg: impl Into<String>, from: impl Into<PgError>) -> Self {
        let mut conv = from.into();
        conv.msg = Some(msg.into());
        conv.title = title;
        conv
    }
}
