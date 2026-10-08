use std::{borrow::Cow, fmt::Display};

use purple_garden_shared::ansi::{
    BLOCK, COMMENT, DEAD, FLOW, FUNC, IMM, LABEL, MNEM, REG, TYPE, paint,
};

use crate::{Const, Func, Id, Instr, Terminator, TypeId};

const MAX_STRING_DISPLAY_CHARS: usize = 65;

fn truncated_string_display(s: &str) -> Cow<'_, str> {
    if s.chars().count() <= MAX_STRING_DISPLAY_CHARS {
        return Cow::Borrowed(s);
    }

    let keep = MAX_STRING_DISPLAY_CHARS - 3;
    let end = s.char_indices().nth(keep).map_or(s.len(), |(idx, _)| idx);
    let mut out = String::with_capacity(end + 3);
    out.push_str(&s[..end]);
    out.push_str("...");
    Cow::Owned(out)
}

impl Display for crate::Fn<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "fn {}(", self.name)?;
        for (i, a) in self.args.iter().enumerate() {
            if let Some(name) = self.arg_names.get(i) {
                write!(f, "{name} ")?;
            }
            if i + 1 < self.args.len() {
                write!(f, "{a} ")?;
            } else {
                write!(f, "{a}")?;
            }
        }
        writeln!(f, ") {}", self.ret)?;
        writeln!(f, "\t{}", self.doc)
    }
}

impl Display for TypeId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.id.0, self.ty)
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Display for Instr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Instr::Store {
                src, base, offset, ..
            } => write!(f, "Store %v{}+{}, %v{}", base, offset, src)?,
            Instr::Load {
                dst, base, offset, ..
            } => write!(f, "%v{dst} = Load %v{}+{}", base, offset)?,
            Instr::AddrOf {
                dst, base, offset, ..
            } => write!(f, "%v{dst} = AddrOf %v{}+{}", base, offset)?,
            Instr::Alloc { dst, layout, .. } => write!(
                f,
                "%v{} = Alloc {}(size={},align={})",
                dst.id,
                dst.ty,
                layout.size(),
                layout.align()
            )?,
            Instr::Bin {
                op, dst, lhs, rhs, ..
            } => write!(f, "%v{} = {:?} %v{}, %v{}", dst, op, lhs.0, rhs.0)?,
            Instr::BinImm {
                op, dst, lhs, imm, ..
            } => write!(f, "%v{} = {:?} %v{}, {}", dst, op, lhs.0, imm)?,
            Instr::LoadConst { dst, value, .. } => write!(f, "%v{dst} = {value}")?,
            Instr::Noop => (),
            Instr::Call {
                dst, func, args, ..
            } => {
                write!(f, "%v{dst} = ")?;
                write!(f, "Call f{}(", func.0)?;
                for (i, arg) in args.iter().enumerate() {
                    if i + 1 == args.len() {
                        write!(f, "%v{}", arg.0)?;
                    } else {
                        write!(f, "%v{}, ", arg.0)?;
                    }
                }
                write!(f, ")")?;
            }
            Instr::Sys {
                dst,
                path,
                fun,
                args,
                ..
            } => {
                write!(f, "%v{dst} = ")?;
                write!(f, "Sys {path}.{}(", fun.name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i + 1 == args.len() {
                        write!(f, "%v{}", arg.0)?;
                    } else {
                        write!(f, "%v{}, ", arg.0)?;
                    }
                }
                write!(f, ")")?;
            }
            Instr::Cast {
                dst: value, from, ..
            } => write!(
                f,
                "%v{} = Cast<{}->{}> %v{}",
                value, from.ty, value.ty, from.id.0
            )?,
            Instr::Lookup {
                dst,
                subject,
                entries,
                default,
                ..
            } => {
                write!(f, "%v{dst} = Lookup %v{} [", subject.0)?;
                for (i, (key, value)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{key} -> {value}")?;
                }
                write!(f, "], {default}")?;
            }
        }
        Ok(())
    }
}

/// Display for a `Terminator` standalone (trace logs). Can't resolve
/// `ParamsId` without a `Func`, so we print the raw pool index as `#N`.
/// The pretty IR dump in `Func`'s Display below resolves it properly.
impl Display for Terminator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Terminator::Return {
                value: Some(id), ..
            } => write!(f, "ret %v{}", id.0)?,
            Terminator::Return { value: None, .. } => write!(f, "ret")?,
            Terminator::Jump { id, params, .. } => write!(f, "jmp b{}(params#{})", id.0, params.0)?,
            Terminator::Branch { cond, yes, no, .. } => write!(
                f,
                "br %v{}, b{}(params#{}), b{}(params#{})",
                cond.0, yes.0, yes.1.0, no.0, no.1.0,
            )?,
            Terminator::BranchCmp {
                op,
                lhs,
                rhs,
                yes,
                no,
                ..
            } => write!(
                f,
                "br_cmp {:?} %v{}, %v{}, b{}(params#{}), b{}(params#{})",
                op, lhs.0, rhs.0, yes.0, yes.1.0, no.0, no.1.0,
            )?,
            Terminator::BranchCmpImm {
                op,
                lhs,
                imm,
                yes,
                no,
                ..
            } => write!(
                f,
                "br_imm {:?} %v{}, {}, b{}(params#{}), b{}(params#{})",
                op, lhs.0, imm, yes.0, yes.1.0, no.0, no.1.0,
            )?,
            Terminator::Switch {
                subject,
                cases,
                default,
                ..
            } => write!(
                f,
                "switch %v{}, cases#{}, b{}(params#{})",
                subject.0, cases.0, default.0.0, default.1.0,
            )?,
            Terminator::Tail { func, args, .. } => {
                write!(f, "tail f{}(", func.0)?;
                for (i, arg) in args.iter().enumerate() {
                    if i + 1 == args.len() {
                        write!(f, "%v{}", arg.0)?;
                    } else {
                        write!(f, "%v{}, ", arg.0)?;
                    }
                }
                write!(f, ")")?;
            }
        }
        Ok(())
    }
}

fn format_ids(ids: &[crate::Id]) -> String {
    ids.iter()
        .map(|p| format!("%v{}", p.0))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One line of the `Func` dump. `pos` is the [`Func::live_set_into`]
/// position of the row, so the liveness view can line intervals up with it.
struct Row {
    pos: Option<u32>,
    indent: usize,
    text: String,
}

impl Func<'_> {
    fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        let mut push = |pos, indent, text| rows.push(Row { pos, indent, text });

        push(None, 0, format!("// {}", self.name));
        push(
            None,
            0,
            format!(
                "fn f{}({}) -> {} {{",
                self.id.0,
                format_ids(&self.params),
                self.ret
                    .as_ref()
                    .map_or_else(|| "Void".to_string(), std::string::ToString::to_string)
            ),
        );

        let mut pos = 0;
        for block in &self.blocks {
            let label = format!(
                "b{}({}):",
                block.id.0,
                format_ids(self.params(block.params))
            );
            if block.tombstone {
                push(None, 1, label);
                push(None, 2, "<tombstone>".to_string());
                continue;
            }
            push(Some(pos), 1, label);
            pos += 2;

            for ins in &block.instructions {
                if !matches!(ins, Instr::Noop) {
                    push(Some(pos), 2, ins.to_string());
                }
                pos += 2;
            }

            match &block.term {
                Some(Terminator::Switch {
                    subject,
                    cases,
                    default,
                    ..
                }) => {
                    push(Some(pos), 2, format!("switch %v{}", subject.0));
                    let arms: Vec<(String, String)> = self
                        .cases(*cases)
                        .iter()
                        .map(|case| (case.key.to_string(), self.target_display(case.target)))
                        .chain([("_".to_string(), self.target_display(*default))])
                        .collect();
                    let width = arms
                        .iter()
                        .map(|(k, _)| k.chars().count())
                        .max()
                        .unwrap_or(0);
                    for (key, target) in arms {
                        push(Some(pos), 3, format!("{key:<width$} -> {target}"));
                    }
                }
                Some(term) => push(Some(pos), 2, self.term_display(term)),
                None => {}
            }
            pos += 2;
        }

        push(None, 0, "}".to_string());
        rows
    }

    fn target_display(&self, (target, params): (Id, crate::ParamsId)) -> String {
        format!("b{}({})", target.0, format_ids(self.params(params)))
    }

    fn term_display(&self, term: &Terminator) -> String {
        match term {
            Terminator::Jump { id, params, .. } => {
                format!("jmp b{}({})", id.0, format_ids(self.params(*params)))
            }
            Terminator::Branch { cond, yes, no, .. } => format!(
                "br %v{}, b{}({}), b{}({})",
                cond.0,
                yes.0,
                format_ids(self.params(yes.1)),
                no.0,
                format_ids(self.params(no.1)),
            ),
            Terminator::BranchCmp {
                op,
                lhs,
                rhs,
                yes,
                no,
                ..
            } => format!(
                "br_cmp {:?} %v{}, %v{}, b{}({}), b{}({})",
                op,
                lhs.0,
                rhs.0,
                yes.0,
                format_ids(self.params(yes.1)),
                no.0,
                format_ids(self.params(no.1)),
            ),
            Terminator::BranchCmpImm {
                op,
                lhs,
                imm,
                yes,
                no,
                ..
            } => format!(
                "br_imm {:?} %v{}, {}, b{}({}), b{}({})",
                op,
                lhs.0,
                imm,
                yes.0,
                format_ids(self.params(yes.1)),
                no.0,
                format_ids(self.params(no.1)),
            ),
            _ => term.to_string(),
        }
    }

    /// The `-I` dump. With `liveness`, every SSA value gets a column on the
    /// right marking the rows its [`Func::live_set_into`] interval overlaps.
    #[must_use]
    pub fn pretty(&self, liveness: bool, color: bool) -> String {
        use std::fmt::Write as _;

        let rows = self.rows();
        let mut intervals = Vec::new();
        if liveness {
            self.live_set_into(&mut intervals);
        }
        // tabs would throw off the gutter alignment
        let indent = if liveness { "    " } else { "\t" };
        let text_width = |r: &Row| r.text.chars().count() + 4 * r.indent;
        let width = rows.iter().map(text_width).max().unwrap_or(0) + 2;
        let labels: Vec<String> = (0..intervals.len()).map(|id| format!("%v{id}")).collect();

        let mut out = String::new();
        for (i, row) in rows.iter().enumerate() {
            out.push_str(&indent.repeat(row.indent));
            if color {
                out.push_str(&colorize(&row.text));
            } else {
                out.push_str(&row.text);
            }
            if !liveness {
                out.push('\n');
                continue;
            }

            let gutter: Option<Vec<String>> = if i == 1 {
                Some(labels.iter().map(|l| paint(color, REG, l)).collect())
            } else {
                row.pos.map(|p| {
                    intervals
                        .iter()
                        .zip(&labels)
                        .map(|(&(def, last), label)| {
                            let pad = " ".repeat(label.len() - 1);
                            if def == u32::MAX || def > p + 1 || last < p {
                                format!(" {pad}")
                            } else if def == last {
                                format!("{}{pad}", paint(color, DEAD, "X"))
                            } else {
                                format!("{}{pad}", paint(color, REG, "|"))
                            }
                        })
                        .collect()
                })
            };
            if let Some(gutter) = gutter {
                let pad = width - text_width(row);
                write!(out, "{:pad$}{}", "", gutter.join(" ")).unwrap();
            }
            out.truncate(out.trim_end().len());
            out.push('\n');
        }
        out
    }
}

/// Colors one rendered row without changing its visible width.
fn colorize(text: &str) -> String {
    const FLOWS: &[&str] = &[
        "br", "br_cmp", "br_imm", "jmp", "ret", "tail", "switch", "Call", "Sys",
    ];

    if text.starts_with("//") || text == "<tombstone>" {
        return paint(true, COMMENT, text);
    }

    let word_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '.');
    let mut out = String::with_capacity(text.len() * 2);
    let mut i = 0;
    while let Some(c) = text[i..].chars().next() {
        let rest = &text[i..];
        let before = &text[..i];
        if c == '`' {
            let end = rest[1..].find('`').map_or(text.len(), |j| i + j + 2);
            out.push_str(&paint(true, IMM, &text[i..end]));
            i = end;
        } else if c == '%' {
            let end = i
                + 1
                + rest[1..]
                    .find(|c: char| !word_char(c))
                    .unwrap_or(rest.len() - 1);
            out.push_str(&paint(true, REG, &text[i..end]));
            i = end;
            if text[i..].starts_with(':') {
                let end = text[i..].find(' ').map_or(text.len(), |j| i + j);
                out.push(':');
                out.push_str(&paint(true, TYPE, &text[i + 1..end]));
                i = end;
            }
        } else if word_char(c)
            || (c == '-'
                && before.ends_with(' ')
                && rest[1..].starts_with(|c: char| c.is_ascii_digit()))
        {
            let end = i
                + 1
                + rest[1..]
                    .find(|c: char| !word_char(c))
                    .unwrap_or(rest.len() - 1);
            let word = &text[i..end];
            let numbered = |p: char| {
                word.len() > 1
                    && word.starts_with(p)
                    && word[1..].bytes().all(|b| b.is_ascii_digit())
            };
            let code = if numbered('b') {
                if i == 0 { BLOCK } else { LABEL }
            } else if numbered('f') || word == "fn" {
                FUNC
            } else if FLOWS.contains(&word) {
                FLOW
            } else if matches!(word, "true" | "false") || !c.is_ascii_alphabetic() {
                IMM
            } else if c.is_ascii_uppercase() && before.trim_end().ends_with(['<', '>']) {
                TYPE
            } else if c.is_ascii_uppercase() {
                MNEM
            } else if word.contains('.') {
                LABEL
            } else {
                ""
            };
            if code.is_empty() {
                out.push_str(word);
            } else {
                out.push_str(&paint(true, code, word));
            }
            i = end;
        } else {
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

impl Display for Func<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for row in self.rows() {
            writeln!(f, "{}{}", "\t".repeat(row.indent), row.text)?;
        }
        Ok(())
    }
}

impl Display for Const<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Const::False => write!(f, "false"),
            Const::True => write!(f, "true"),
            Const::Int(int) => write!(f, "{int}"),
            Const::Double(bits) => write!(f, "{}", f64::from_bits(*bits)),
            Const::Str(str) => write!(f, "`{}`", truncated_string_display(str)),
            Const::Undefined => unreachable!(),
        }
    }
}
