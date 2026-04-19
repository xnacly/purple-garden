use crate::{
    ast::TypeExpr,
    lex::{Token, Type},
    vm::Anomaly,
};

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
        let title = match value {
            Anomaly::Msg { msg, .. } => msg,
            _ => "Virtual Machine Anomaly",
        };
        PgError {
            msg: value.as_str().to_string(),
            line: 0,
            start: 0,
            len: 0,
        }
    }
}

impl PgError {
    // TODO: introduce a writer to write errors to?
    pub fn render(self, file: &str, lines: &[&str]) {
        println!("{file}:{}:{}: {}", self.line, self.start, self.msg);

        if let Some(line) = lines.get(self.line) {
            println!("{line}");
            println!("{}~", " ".repeat(self.start))
        }
    }

    pub fn with_msg(msg: impl Into<String>, from: impl Into<PgError>) -> Self {
        let mut conv = from.into();
        conv.msg = msg.into();
        conv
    }
}
