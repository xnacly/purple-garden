//! The lines of the listings: the IR of a pass diffed against the one before it, bytecode, x86-64
//! and plain dumps. Every line carries its highlights and the liveness row it maps to, so any
//! renderer links them alike.

use std::ops::Range;

use purple_garden_shared::ansi::Class;
use serde::{Deserialize, Serialize};
use similar::{Algorithm, DiffOp, capture_diff_slices};

use crate::{BcLine, BcLineKind, Function, Inspection, IrRow};

/// How a [`Line`] stands out from the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Tone {
    #[default]
    Normal = 0,
    /// A function or block label.
    Label,
    /// Added by the pass shown.
    Added,
    /// Removed by the pass shown.
    Removed,
    /// Inlined data, frames and other lines nothing maps to.
    Muted,
}

impl Tone {
    /// Every tone, in declaration order: `Tone::ALL[tone as usize] == tone`.
    pub const ALL: [Tone; 5] = [
        Tone::Normal,
        Tone::Label,
        Tone::Added,
        Tone::Removed,
        Tone::Muted,
    ];
}

/// A liveness row of a function: the function's id and the row.
pub type RowRef = (u32, u32);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub gutter: String,
    pub text: String,
    pub highlights: Vec<(Range<usize>, Class)>,
    /// The liveness row the line maps to.
    pub row: Option<RowRef>,
    pub tone: Tone,
}

fn line(func: u32, row: &IrRow, tone: Tone) -> Line {
    let indent = "  ".repeat(row.indent as usize);
    let text = format!("{indent}{}", row.text);
    let highlights = purple_garden_ir::highlight_row(&row.text)
        .into_iter()
        .map(|(r, c)| (r.start + indent.len()..r.end + indent.len(), c))
        .collect();
    let tone = match tone {
        Tone::Normal if row.indent <= 1 && row.pos.is_some() => Tone::Label,
        tone => tone,
    };
    Line {
        gutter: row.pos.map(|p| p.to_string()).unwrap_or_default(),
        text,
        highlights,
        row: row
            .pos
            .filter(|_| tone != Tone::Removed)
            .map(|pos| (func, pos)),
        tone,
    }
}

/// The rows after `pass`, a line per row.
#[must_use]
pub fn plain(f: &Function, pass: usize) -> Vec<Line> {
    f.ir_at(pass)
        .rows
        .iter()
        .map(|r| line(f.id, r, Tone::Normal))
        .collect()
}

/// A run of rows of a pass' diff against the pass before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Change {
    /// `len` rows both have, from `new` on in the IR after the pass.
    Kept { new: usize, len: usize },
    /// `len` rows the pass removed, from `old` on in the IR before it.
    Removed { old: usize, len: usize },
    /// `len` rows the pass added, from `new` on in the IR after it.
    Added { new: usize, len: usize },
}

/// How `pass` changed `f`, `None` if it didn't.
#[must_use]
pub fn changes(f: &Function, pass: usize) -> Option<Vec<Change>> {
    if pass == 0 || f.snapshots[pass].ir.is_none() {
        return None;
    }
    let texts = |pass| {
        f.ir_at(pass)
            .rows
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
    };
    let (old, new) = (texts(pass - 1), texts(pass));
    let mut out = Vec::new();
    for op in capture_diff_slices(Algorithm::Myers, &old, &new) {
        match op {
            DiffOp::Equal { new_index, len, .. } => out.push(Change::Kept {
                new: new_index,
                len,
            }),
            DiffOp::Delete {
                old_index, old_len, ..
            } => out.push(Change::Removed {
                old: old_index,
                len: old_len,
            }),
            DiffOp::Insert {
                new_index, new_len, ..
            } => out.push(Change::Added {
                new: new_index,
                len: new_len,
            }),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => out.extend([
                Change::Removed {
                    old: old_index,
                    len: old_len,
                },
                Change::Added {
                    new: new_index,
                    len: new_len,
                },
            ]),
        }
    }
    Some(out)
}

/// The rows after `pass`, with the rows it removed in between, marked.
#[must_use]
pub fn diffed(f: &Function, pass: usize) -> Vec<Line> {
    let Some(changes) = changes(f, pass) else {
        return plain(f, pass);
    };
    let (old, new) = (&f.ir_at(pass - 1).rows, &f.ir_at(pass).rows);
    let mut out = Vec::with_capacity(new.len());
    for change in changes {
        let (rows, tone) = match change {
            Change::Kept { new: at, len } => (&new[at..at + len], Tone::Normal),
            Change::Removed { old: at, len } => (&old[at..at + len], Tone::Removed),
            Change::Added { new: at, len } => (&new[at..at + len], Tone::Added),
        };
        out.extend(rows.iter().map(|r| line(f.id, r, tone)));
    }
    out
}

/// The bytecode of `functions`, labels included, `None` for all of it.
#[must_use]
pub fn bytecode_lines(inspection: &Inspection, functions: Option<&[&Function]>) -> Vec<Line> {
    inspection
        .bytecode
        .iter()
        .filter(|l| functions.is_none_or(|fs| fs.iter().any(|f| l.func == Some(f.id))))
        .map(bytecode_line)
        .collect()
}

fn bytecode_line(l: &BcLine) -> Line {
    match l.kind {
        BcLineKind::Func => Line {
            gutter: format!("{:04x}", l.pc),
            text: format!("<{}>:", l.text),
            highlights: vec![(0..l.text.len() + 3, Class::Func)],
            tone: Tone::Label,
            ..Line::default()
        },
        BcLineKind::Block => Line {
            gutter: format!("{:04x}", l.pc),
            text: format!("<{}>:", l.text),
            highlights: vec![(0..l.text.len() + 3, Class::Block)],
            tone: Tone::Label,
            ..Line::default()
        },
        BcLineKind::Op => Line {
            gutter: format!("{:04x}", l.pc),
            highlights: purple_garden_bc::dis::highlight_instr(&l.text)
                .into_iter()
                .map(|(r, c)| (r.start + 2..r.end + 2, c))
                .collect(),
            text: format!("  {}", l.text),
            row: l.func.zip(l.row),
            tone: Tone::Normal,
        },
    }
}

/// The x86-64 code of `functions`, a function without names why there is none.
#[must_use]
pub fn x86_lines(functions: &[&Function]) -> Vec<Line> {
    let mut out = Vec::new();
    for f in functions {
        if !out.is_empty() {
            out.push(Line::default());
        }
        match function_x86_lines(f) {
            Ok(lines) => out.extend(lines),
            Err(reason) => {
                let label = format!("f{} <{}>", f.id, f.name);
                out.push(Line {
                    text: format!("{label}:"),
                    highlights: vec![(0..label.len() + 1, Class::Func)],
                    tone: Tone::Label,
                    ..Line::default()
                });
                let text = format!("  ; stays bytecode: {reason}");
                out.push(Line {
                    highlights: vec![(0..text.len(), Class::Comment)],
                    text,
                    tone: Tone::Muted,
                    ..Line::default()
                });
            }
        }
    }
    out
}

/// The x86-64 code of `f`, or why there is none.
fn function_x86_lines(f: &Function) -> Result<Vec<Line>, String> {
    let native = f.x86.as_ref().map_err(Clone::clone)?;
    let mut out = Vec::with_capacity(native.lines.len() * 5 / 4);
    for l in &native.lines {
        if let Some(label) = &l.label {
            out.push(Line {
                gutter: format!("{:04x}", l.offset),
                text: format!("{label}:"),
                highlights: vec![(
                    0..label.len() + 1,
                    if label.starts_with('b') {
                        Class::Block
                    } else {
                        Class::Func
                    },
                )],
                tone: Tone::Label,
                ..Line::default()
            });
        }
        let text = format!("  {:<30} {}", l.bytes, l.text);
        let at = 2 + 30.max(l.bytes.len()) + 1;
        let mut highlights = vec![(2..2 + l.bytes.len(), Class::Addr)];
        if !l.data {
            highlights.extend(
                x86_highlights(&l.text)
                    .into_iter()
                    .map(|(r, c)| (r.start + at..r.end + at, c)),
            );
        }
        out.push(Line {
            gutter: format!("{:04x}", l.offset),
            text,
            highlights,
            row: l.row.map(|row| (f.id, row)),
            tone: if l.data || l.row.is_none() {
                Tone::Muted
            } else {
                Tone::Normal
            },
        });
    }
    Ok(out)
}

/// Highlights of an intel syntax instruction: mnemonic, registers, immediates and labels.
#[must_use]
pub fn x86_highlights(text: &str) -> Vec<(std::ops::Range<usize>, Class)> {
    let mnem_end = text.find(' ').unwrap_or(text.len());
    let mnem = &text[..mnem_end];
    let flow = matches!(mnem, "jmp" | "call" | "ret" | "int3") || mnem.starts_with('j');
    let mut out = vec![(0..mnem_end, if flow { Class::Flow } else { Class::Mnem })];
    let operands = text[mnem_end..].trim_start();
    let at = text.len() - operands.len();
    // a branch target the symbol resolver named: `b3`, `fib+0x88`, `f1 <fib>`
    if flow && !operands.is_empty() && !operands.contains('[') && !operands.starts_with("0x") {
        let register = operands.starts_with('r') || operands.starts_with('e');
        if !register || operands.contains('+') || operands.contains('<') {
            out.push((at..text.len(), Class::Label));
            return out;
        }
    }
    let bytes = text.as_bytes();
    let mut i = at;
    while i < bytes.len() {
        if !(bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        let word = &text[start..i];
        let class = if word.starts_with(|c: char| c.is_ascii_digit()) {
            Class::Imm
        } else if matches!(word, "qword" | "dword" | "word" | "byte" | "ptr") {
            Class::Comment
        } else {
            Class::Reg
        };
        out.push((start..i, class));
    }
    out
}

/// A plain text dump, one line per line.
#[must_use]
pub fn text_lines(text: &str) -> Vec<Line> {
    text.lines()
        .enumerate()
        .map(|(i, l)| Line {
            gutter: (i + 1).to_string(),
            text: l.to_string(),
            ..Line::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    mod ir {
        use purple_garden_shared::config::Config;

        use super::super::{Tone, diffed};

        #[test]
        fn diffs_mark_what_a_pass_changed() {
            let i = crate::inspect("test.garden", b"fn f(a:Int) Int { a + 2 * 3 }\nfn g(a:Int) Int { match { a == 0 { 0 } { f(a) + g(a - 1) } } }\ng(4)",
            &Config::default(),
        );
            let g = i.functions.iter().find(|f| f.name == "g").unwrap();
            let mut seen_change = false;
            for pass in 0..g.snapshots.len() {
                let lines = diffed(g, pass);
                let kept = lines.iter().filter(|l| l.tone != Tone::Removed).count();
                assert_eq!(
                    kept,
                    g.ir_at(pass).rows.len(),
                    "pass {pass} keeps every row"
                );
                let changed = lines
                    .iter()
                    .any(|l| matches!(l.tone, Tone::Added | Tone::Removed));
                if pass == 0 || g.snapshots[pass].ir.is_none() {
                    assert!(!changed, "pass {pass} changed nothing");
                }
                seen_change |= changed;
                assert!(
                    lines
                        .iter()
                        .filter(|l| l.tone == Tone::Removed)
                        .all(|l| l.row.is_none()),
                    "removed rows link nowhere"
                );
            }
            assert!(seen_change, "inlining f into g shows up");
        }
    }

    mod code {
        use purple_garden_shared::ansi::Class;

        use super::super::x86_highlights;

        #[test]
        fn x86_highlights_tell_registers_from_immediates_and_labels() {
            let text = "call    f1 <fib>";
            let h = x86_highlights(text);
            let words: Vec<_> = h.iter().map(|(r, c)| (&text[r.clone()], *c)).collect();
            assert_eq!(words, [("call", Class::Flow), ("f1 <fib>", Class::Label)]);

            let text = "mov     rax, qword ptr [rbx+0x1e8]";
            let words: Vec<_> = x86_highlights(text)
                .into_iter()
                .map(|(r, c)| (&text[r], c))
                .collect();
            assert_eq!(words[0], ("mov", Class::Mnem));
            assert!(words.contains(&("rax", Class::Reg)));
            assert!(words.contains(&("qword", Class::Comment)));
            assert!(words.contains(&("rbx", Class::Reg)));
            assert!(words.contains(&("0x1e8", Class::Imm)));

            let text = "call    rax";
            let words: Vec<_> = x86_highlights(text)
                .into_iter()
                .map(|(r, c)| (&text[r], c))
                .collect();
            assert_eq!(words, [("call", Class::Flow), ("rax", Class::Reg)]);

            let text = "jmp     fib+0x88";
            let words: Vec<_> = x86_highlights(text)
                .into_iter()
                .map(|(r, c)| (&text[r], c))
                .collect();
            assert_eq!(words, [("jmp", Class::Flow), ("fib+0x88", Class::Label)]);

            let text = "jne     b2";
            let words: Vec<_> = x86_highlights(text)
                .into_iter()
                .map(|(r, c)| (&text[r], c))
                .collect();
            assert_eq!(words, [("jne", Class::Flow), ("b2", Class::Label)]);
            for text in ["", "ret", "jmp     fib+0x88"] {
                for (r, _) in x86_highlights(text) {
                    assert!(r.end <= text.len());
                }
            }
        }
    }
}
