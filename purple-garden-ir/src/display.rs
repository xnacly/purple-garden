use std::{borrow::Cow, fmt::Display};

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
        write!(f, "{}:{:?}", self.id.0, self.ty)
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
            Instr::Alloc { dst, .. } => {
                let layout = dst.ty.layout();
                write!(
                    f,
                    "%v{} = Alloc {}(size={},align={})",
                    dst.id,
                    dst.ty,
                    layout.size(),
                    layout.align()
                )?
            }
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

impl Display for Func<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "// {}\nfn f{}(", self.name, self.id.0)?;
        for (i, arg) in self.params.iter().enumerate() {
            if i + 1 == self.params.len() {
                write!(f, "%v{arg}")?;
            } else {
                write!(f, "%v{arg}, ")?;
            }
        }

        writeln!(
            f,
            ") -> {} {{",
            self.ret
                .as_ref()
                .map_or_else(|| "Void".to_string(), std::string::ToString::to_string)
        )?;

        for block in &self.blocks {
            writeln!(
                f,
                "b{}({}):",
                block.id.0,
                format_ids(self.params(block.params))
            )?;

            if block.tombstone {
                writeln!(f, "\t<tombstone>")?;
                continue;
            }

            for ins in &block.instructions {
                if let Instr::Noop = ins {
                    continue;
                };
                writeln!(f, "\t{ins}")?;
            }

            if let Some(term) = &block.term {
                match term {
                    Terminator::Jump { id, params, .. } => {
                        writeln!(f, "\tjmp b{}({})", id.0, format_ids(self.params(*params)))?;
                    }
                    Terminator::Branch { cond, yes, no, .. } => writeln!(
                        f,
                        "\tbr %v{}, b{}({}), b{}({})",
                        cond.0,
                        yes.0,
                        format_ids(self.params(yes.1)),
                        no.0,
                        format_ids(self.params(no.1)),
                    )?,
                    Terminator::BranchCmpImm {
                        op,
                        lhs,
                        imm,
                        yes,
                        no,
                        ..
                    } => writeln!(
                        f,
                        "\tbr_imm {:?} %v{}, {}, b{}({}), b{}({})",
                        op,
                        lhs.0,
                        imm,
                        yes.0,
                        format_ids(self.params(yes.1)),
                        no.0,
                        format_ids(self.params(no.1)),
                    )?,
                    _ => writeln!(f, "\t{term}")?,
                }
            }
        }

        writeln!(f, "}}")
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
