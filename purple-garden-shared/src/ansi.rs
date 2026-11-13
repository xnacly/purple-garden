//! Highlight classes shared by the IR and bytecode dumps, painted as ANSI SGR in the terminal, by
//! other renderers in their own colors.

use std::{io::IsTerminal, ops::Range};

/// Byte ranges of a text and their [`Class`], ascending and non-overlapping.
pub type Highlights = Vec<(Range<usize>, Class)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Class {
    /// pc / addresses
    Addr,
    /// data mnemonics
    Mnem,
    /// control-flow mnemonics
    Flow,
    /// registers
    Reg,
    /// immediates
    Imm,
    /// jump targets / symbols
    Label,
    /// source/value annotations
    Comment,
    /// function headers
    Func,
    /// basic-block labels
    Block,
    /// section headers
    Section,
    /// types
    Type,
    /// defined but never used
    Dead,
}

impl Class {
    /// Every class, in declaration order: `Class::ALL[class as usize] == class`.
    pub const ALL: [Class; 12] = [
        Class::Addr,
        Class::Mnem,
        Class::Flow,
        Class::Reg,
        Class::Imm,
        Class::Label,
        Class::Comment,
        Class::Func,
        Class::Block,
        Class::Section,
        Class::Type,
        Class::Dead,
    ];

    /// The SGR parameters of the class.
    #[must_use]
    pub const fn sgr(self) -> &'static str {
        match self {
            Class::Addr | Class::Comment => "90", // gray
            Class::Mnem => "36",                  // cyan
            Class::Flow => "1;35",                // bold magenta
            Class::Reg => "32",                   // green
            Class::Imm => "33",                   // yellow
            Class::Label => "94",                 // blue
            Class::Func => "1;33",                // bold yellow
            Class::Block => "1;94",               // bold blue
            Class::Section => "1",                // bold
            Class::Type => "35",                  // magenta
            Class::Dead => "1;31",                // bold red
        }
    }
}

/// Colors only when stdout is a tty and `NO_COLOR` is unset.
#[must_use]
pub fn enabled() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

#[must_use]
pub fn paint(on: bool, class: Class, s: &str) -> String {
    if on {
        format!("\x1b[{}m{s}\x1b[0m", class.sgr())
    } else {
        s.to_string()
    }
}

/// `text` with every highlighted range painted, the rest left as is.
#[must_use]
pub fn paint_highlights(on: bool, text: &str, highlights: &[(Range<usize>, Class)]) -> String {
    if !on {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() * 2);
    let mut at = 0;
    for (range, class) in highlights {
        out.push_str(&text[at..range.start]);
        out.push_str(&paint(true, *class, &text[range.clone()]));
        at = range.end;
    }
    out.push_str(&text[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_lists_every_class_at_its_discriminant() {
        for (i, class) in Class::ALL.iter().enumerate() {
            assert_eq!(*class as usize, i);
        }
    }

    #[test]
    fn paint_highlights_keeps_unhighlighted_text() {
        let text = "mov r1, r2";
        let highlights = vec![(0..3, Class::Mnem), (4..6, Class::Reg), (8..10, Class::Reg)];
        assert_eq!(
            paint_highlights(true, text, &highlights),
            "\x1b[36mmov\x1b[0m \x1b[32mr1\x1b[0m, \x1b[32mr2\x1b[0m"
        );
        assert_eq!(paint_highlights(false, text, &highlights), text);
        assert_eq!(paint_highlights(true, "", &[]), "");
    }
}
