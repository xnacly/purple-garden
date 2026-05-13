use crate::{ast::TypeExpr, lex::Token, vm::Anomaly};
use std::fmt::Write;

#[derive(Debug)]
pub struct PgError {
    pub msg: String,
    /// Byte offset into the source where the offending region starts
    pub start: usize,
    pub len: usize,
}

impl From<&Token<'_>> for PgError {
    fn from(value: &Token) -> Self {
        PgError {
            msg: String::new(),
            start: value.start,
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
            start: 0,
            len: 0,
        }
    }
}

impl PgError {
    pub fn render(self, file: &str, source: &[u8]) -> String {
        // TODO: replace this with a proper SourceMap that prebuilds a sorted
        // Vec<usize> of newline byte offsets at parse start, then maps a
        // byte offset to (line, col_bytes) via binary search and slices the
        // line text out of `source` in O(log n) instead of the O(n) scan
        // below. The scan below is fine for the rare error path but would
        // not scale to LSP-style repeated diagnostics on a large file.
        //
        // Note: `col` here is a byte column, not a grapheme column — proper
        // unicode-aware column tracking belongs in the same SourceMap.
        let (line_no, col, line_text) = locate(source, self.start);

        let mut buf = String::new();
        writeln!(&mut buf, "{file}:{line_no}:{col}: {}:", self.msg).unwrap();
        writeln!(&mut buf, "{line_text}").unwrap();
        writeln!(
            &mut buf,
            "{}{}",
            " ".repeat(col),
            "~".repeat(self.len.max(1))
        )
        .unwrap();
        buf
    }

    pub fn with_msg(msg: impl Into<String>, from: impl Into<PgError>) -> Self {
        let mut conv = from.into();
        conv.msg = msg.into();
        conv
    }
}

fn locate(source: &[u8], offset: usize) -> (usize, usize, &str) {
    let clamped = offset.min(source.len());
    let prefix = &source[..clamped];
    let line_no = prefix.iter().filter(|&&b| b == b'\n').count();
    let line_start = prefix
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    let col = clamped - line_start;
    let line_end = source[line_start..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(source.len(), |i| line_start + i);
    let line_text = std::str::from_utf8(&source[line_start..line_end]).unwrap_or("<invalid utf-8>");
    (line_no, col, line_text)
}
