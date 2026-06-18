//! Frontend diagnostics.
//!
//! A diagnostic is the shared representation for parser, typechecker, CLI, and
//! future LSP errors. The frontend does not model warnings or informational
//! diagnostics: if Purple Garden cares enough to report something, it is an
//! error. Context and actions live inside the error as notes and helps.
//!
//! Keep this module data-first. The parser and typechecker should build
//! structured diagnostics; terminal rendering and LSP conversion are views over
//! the same data.

use crate::lex::Token;
use purple_garden_runtime::{Anomaly, DebugInfo};
use std::fmt::Write;

/// A byte span in a source file.
///
/// Both `start` and `len` are byte counts, not character or grapheme counts.
/// This matches the lexer and keeps spans cheap to carry through the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Byte offset into the source where the region starts.
    pub start: usize,
    /// Byte length of the region. Renderers should treat `0` as a point span.
    pub len: usize,
}

impl Span {
    /// Build a span from raw byte positions.
    #[must_use]
    pub fn new(start: usize, len: usize) -> Self {
        Self { start, len }
    }

    /// Span the bytes occupied by a token's textual representation.
    #[must_use]
    pub fn from_token(token: &Token<'_>) -> Self {
        Self::new(token.start, token.t.as_str().len())
    }
}

/// A source label attached to a diagnostic.
///
/// The primary label is where the error is anchored. Secondary labels are kept
/// for later multi-span diagnostics; the current terminal renderer only prints
/// the primary label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub span: Span,
    pub message: Option<String>,
}

impl Label {
    /// Build an unlabeled source marker.
    #[must_use]
    pub fn new(span: Span) -> Self {
        Self {
            span,
            message: None,
        }
    }

    /// Add short text that should be rendered next to the underline.
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

/// A source edit associated with a help message.
///
/// This is not rendered by the terminal formatter yet, but it gives the future
/// LSP adapter enough information to expose code actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    pub span: Span,
    pub text: String,
}

/// Actionable guidance attached to a diagnostic.
///
/// Use notes for context and helps for actions. For example, "package `strings`
/// exists but is not imported" is a note; "add `import \"strings\"`" is help.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Help {
    pub message: String,
    pub replacement: Option<Replacement>,
}

impl Help {
    /// Build a help message without a machine-applicable edit.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            replacement: None,
        }
    }

    /// Attach a replacement edit to this help.
    #[must_use]
    pub fn with_replacement(mut self, span: Span, text: impl Into<String>) -> Self {
        self.replacement = Some(Replacement {
            span,
            text: text.into(),
        });
        self
    }
}

/// A frontend error diagnostic.
///
/// `message` is the headline. `primary` is the main source location. `notes`
/// provide context and `helps` describe actions the user can take. There is no
/// severity field because Purple Garden only reports errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub primary: Label,
    pub secondary: Vec<Label>,
    pub notes: Vec<String>,
    pub helps: Vec<Help>,
}

impl From<&Token<'_>> for Diagnostic {
    fn from(value: &Token) -> Self {
        Diagnostic::new(String::new(), Span::from_token(value))
    }
}

impl Diagnostic {
    /// Build a diagnostic with a primary span and no label text.
    #[must_use]
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            primary: Label::new(span),
            secondary: Vec::new(),
            notes: Vec::new(),
            helps: Vec::new(),
        }
    }

    /// Build a diagnostic whose primary span is a token.
    #[must_use]
    pub fn at_token(message: impl Into<String>, token: &Token<'_>) -> Self {
        Self::new(message, Span::from_token(token))
    }

    /// Build a diagnostic from a VM trap. The runtime hands back the trap
    /// `pc` only; the source byte offset is resolved here by consulting
    /// the compile-time `DebugInfo`. Keeps `Vm` free of source-info
    /// bookkeeping (the runtime hot path never reads `DebugInfo`).
    #[must_use]
    pub fn from_anomaly(anomaly: Anomaly, debug: &DebugInfo) -> Self {
        Self::new(
            anomaly.as_str().to_string(),
            Span::new(debug.span_at(anomaly.pc()) as usize, 0),
        )
    }

    /// Render this diagnostic for the command-line interface.
    ///
    /// This renderer is intentionally small and consumes `self` so callers can
    /// build diagnostics with owned strings without cloning them for display.
    /// LSP integration should consume the same fields directly instead of
    /// parsing this text.
    #[must_use]
    pub fn render(self, file: &str, source: &[u8]) -> String {
        let location = locate(source, self.primary.span.start);

        let mut buf = String::new();
        writeln!(
            &mut buf,
            "{file}:{}:{}: {}:",
            location.line_no + 1,
            location.col + 1,
            self.message
        )
        .unwrap();
        render_primary_label(&mut buf, location, self.primary);

        for note in self.notes {
            writeln!(&mut buf, "note: {note}").unwrap();
        }
        for help in self.helps {
            writeln!(&mut buf, "help: {}", help.message).unwrap();
        }
        buf
    }

    /// Add short text next to the primary underline.
    #[must_use]
    pub fn with_primary_message(mut self, message: impl Into<String>) -> Self {
        self.primary = self.primary.with_message(message);
        self
    }

    /// Add contextual information. Notes should not tell the user what to do.
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Add an actionable suggestion.
    #[must_use]
    pub fn with_help(mut self, help: Help) -> Self {
        self.helps.push(help);
        self
    }
}

#[derive(Clone, Copy)]
struct Location<'src> {
    /// 0-based line number.
    line_no: usize,
    /// 0-based byte column.
    col: usize,
    line_text: &'src str,
}

fn render_primary_label(buf: &mut String, location: Location<'_>, label: Label) {
    writeln!(buf, "{}", location.line_text).unwrap();
    write!(
        buf,
        "{}{}",
        " ".repeat(location.col),
        "~".repeat(label.span.len.max(1))
    )
    .unwrap();
    if let Some(message) = label.message {
        write!(buf, " {message}").unwrap();
    }
    writeln!(buf).unwrap();
}

fn locate(source: &[u8], offset: usize) -> Location<'_> {
    // TODO: replace this with a proper SourceMap that prebuilds a sorted
    // Vec<usize> of newline byte offsets at parse start, then maps a byte
    // offset to (line, col_bytes) via binary search and slices the line text
    // out of `source` in O(log n) instead of the O(n) scan below. The scan is
    // fine for fail-fast CLI errors but not for repeated LSP diagnostics.
    //
    // `col` is a byte column, not a grapheme column. Unicode-aware display
    // columns belong in the same SourceMap.
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
    Location {
        line_no,
        col,
        line_text,
    }
}
