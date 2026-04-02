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

impl Display for Instr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Instr::Bin { op, dst, lhs, rhs } => {
                write!(f, "%v{} = {:?} %v{}, %v{}", dst, op, lhs.0, rhs.0)?
            }
            Instr::LoadConst { dst, value } => write!(f, "%v{} = {}", dst, value)?,
            Instr::Noop => write!(f, "Nop")?,
            Instr::Call { dst, func, args } => {
                write!(f, "%v{} = ", dst)?;
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
                func,
                args,
            } => {
                write!(f, "%v{} = ", dst)?;
                write!(f, "Sys {path}.{}(", func.name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i + 1 == args.len() {
                        write!(f, "%v{}", arg.0)?;
                    } else {
                        write!(f, "%v{}, ", arg.0)?;
                    }
                }
                write!(f, ")")?;
            }
            Instr::Tail { dst, func, args } => {
                write!(f, "%v{} = ", dst)?;
                write!(f, "Tailcall f{}(", func.0)?;
                for (i, arg) in args.iter().enumerate() {
                    if i + 1 == args.len() {
                        write!(f, "%v{}", arg.0)?;
                    } else {
                        write!(f, "%v{}, ", arg.0)?;
                    }
                }
                write!(f, ")")?;
            }
            Instr::Cast { dst: value, from } => {
                write!(f, "%v{} = Cast_to_{} %v{}", value, value.ty, from.0)?
            }
        }
        Ok(())
    }
}

impl Display for Terminator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Terminator::Return(Some(id)) => write!(f, "ret %v{}", id.0)?,
            Terminator::Return(None) => write!(f, "ret")?,
            Terminator::Jump { id, params } => {
                if params.is_empty() {
                    write!(f, "jmp b{}", id.0)?
                } else {
                    write!(
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
            Terminator::Branch { cond, yes, no } => write!(
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
        Ok(())
    }
}

impl Display for Func<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let entry_block = self
            .blocks
            .first()
            .expect("Func.entry does not reference a valid block");

        write!(f, "// {}\nfn f{}(", self.name, self.id.0)?;
        for (i, arg) in self.params.iter().enumerate() {
            if i + 1 == self.params.len() {
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
                writeln!(f, "\t{ins}")?;
            }

            if let Some(term) = &block.term {
                writeln!(f, "\t{term}")?;
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
            _ => unreachable!(),
        }
    }
}
