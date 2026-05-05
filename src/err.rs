use crate::{ast::TypeExpr, lex::Token, vm::Anomaly};
use std::fmt::Write;

#[derive(Debug)]
pub struct PgError {
    pub msg: String,
    pub line: usize,
    pub start: usize,
    pub len: usize,
}

impl From<&Token<'_>> for PgError {
    fn from(value: &Token) -> Self {
        PgError {
            msg: String::new(),
            line: value.line,
            start: value.col,
            len: value.t.as_str().len(),
        }
    }
}

impl From<&TypeExpr<'_>> for PgError {
    fn from(value: &TypeExpr<'_>) -> Self {
        match value {
            TypeExpr::Atom(tok) => tok.into(),
            TypeExpr::Option(inner) | TypeExpr::Array(inner) => inner.as_ref().into(),
        }
    }
}

impl From<Anomaly> for PgError {
    fn from(value: Anomaly) -> Self {
        // TODO: do some prep in anomaly for finding out which ast node resulted in what bytecode
        // ranges
        PgError {
            msg: value.as_str().to_string(),
            line: 0,
            start: 0,
            len: 0,
        }
    }
}

impl PgError {
    pub fn render(self, file: &str, lines: &[&str]) -> String {
        let mut buf = String::new();
        writeln!(
            &mut buf,
            "{file}:{}:{}: {}:",
            self.line, self.start, self.msg
        )
        .unwrap();

        if let Some(line) = lines.get(self.line) {
            writeln!(&mut buf, "{line}").unwrap();
            writeln!(
                &mut buf,
                "{}{}",
                " ".repeat(self.start),
                "~".repeat(self.len.max(1))
            )
            .unwrap();
        };

        buf
    }

    pub fn with_msg(msg: impl Into<String>, from: impl Into<PgError>) -> Self {
        let mut conv = from.into();
        conv.msg = msg.into();
        conv
    }
}
