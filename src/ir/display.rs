use std::fmt::Display;

use crate::ir::{Const, Func, Id, Instr, Terminator, TypeId};

impl Display for TypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{:?}", self.id.0, self.ty)
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Display for Func<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let entry_block = self
            .blocks
            .first()
            .expect("Func.entry does not reference a valid block");

        write!(f, "// {}\nfn f{}(", self.name, self.id.0)?;
        for (i, arg) in entry_block.params.iter().enumerate() {
            if i + 1 == entry_block.params.len() {
                write!(f, "%v{}", arg)?;
            } else {
                write!(f, "%v{}, ", arg)?;
            }
        }
        writeln!(
            f,
            ") -> {} {{",
            self.ret
                .as_ref()
                .map(|t| t.to_string())
                .unwrap_or_else(|| "void".to_string())
        )?;

        for block in self.blocks.iter() {
            writeln!(
                f,
                "b{}({}):",
                block.id.0,
                block
                    .params
                    .iter()
                    .map(|p| format!("%v{}", p))
                    .collect::<Vec<_>>()
                    .join(", ")
            )?;

            if block.tombstone {
                writeln!(f, "\t<tombstone>")?;
                continue;
            }

            for ins in &block.instructions {
                write!(f, "\t")?;
                match ins {
                    Instr::Bin { op, dst, lhs, rhs } => {
                        writeln!(f, "%v{} = {:?} %v{}, %v{}", dst, op, lhs.0, rhs.0)?
                    }
                    Instr::LoadConst { dst, value } => writeln!(f, "%v{} = {}", dst, value)?,
                    Instr::Noop => writeln!(f, "nop")?,
                    Instr::Call { dst, func, args } => {
                        write!(f, "%v{} = ", dst)?;
                        write!(f, "f{}(", func.0)?;
                        for (i, arg) in args.iter().enumerate() {
                            if i + 1 == args.len() {
                                write!(f, "%v{}", arg.0)?;
                            } else {
                                write!(f, "%v{}, ", arg.0)?;
                            }
                        }
                        writeln!(f, ")")?;
                    }
                    Instr::Tail { dst, func, args } => {
                        write!(f, "%v{} = ", dst)?;
                        write!(f, "f_tail{}(", func.0)?;
                        for (i, arg) in args.iter().enumerate() {
                            if i + 1 == args.len() {
                                write!(f, "%v{}", arg.0)?;
                            } else {
                                write!(f, "%v{}, ", arg.0)?;
                            }
                        }
                        writeln!(f, ")")?;
                    }
                    Instr::Cast { dst: value, from } => {
                        writeln!(f, "%v{} = cast_to_{} %v{}", value, value.ty, from.0)?
                    }
                }
            }

            if let Some(term) = &block.term {
                write!(f, "\t")?;
                match &term {
                    Terminator::Return(Some(id)) => writeln!(f, "ret %v{}", id.0)?,
                    Terminator::Return(None) => writeln!(f, "ret")?,
                    Terminator::Jump { id, params } => {
                        if params.is_empty() {
                            writeln!(f, "jmp b{}", id.0)?
                        } else {
                            writeln!(
                                f,
                                "jmp b{}({})",
                                id.0,
                                params
                                    .iter()
                                    .map(|p| format!("%v{}", p.0))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )?
                        }
                    }
                    Terminator::Branch { cond, yes, no } => writeln!(
                        f,
                        "br %v{}, b{}({}), b{}({})",
                        cond.0,
                        yes.0,
                        yes.1
                            .iter()
                            .map(|p| format!("%v{}", p.0))
                            .collect::<Vec<_>>()
                            .join(", "),
                        no.0,
                        no.1.iter()
                            .map(|p| format!("%v{}", p.0))
                            .collect::<Vec<_>>()
                            .join(", "),
                    )?,
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
            Const::Str(str) => write!(f, "`{str}`"),
        }
    }
}
