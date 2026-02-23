use std::fmt::Display;

use crate::ir::{Const, Func, Instr, Terminator, TypeId};

impl Display for TypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{:?}", self.id.0, self.ty)
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
            if block.params.is_empty() || block.id == entry_block.id {
                writeln!(f, "b{}:", block.id.0)?;
            } else {
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
            }

            if block.tombstone {
                writeln!(f, "<tombstone>")?;
                continue;
            }

            for ins in &block.instructions {
                write!(f, "\t")?;
                match ins {
                    Instr::Noop => writeln!(f, "nop")?,
                    Instr::Add { dst, lhs, rhs } => {
                        writeln!(f, "%v{} = add %v{}, %v{}", dst, lhs.0, rhs.0)?
                    }
                    Instr::Sub { dst, lhs, rhs } => {
                        writeln!(f, "%v{} = sub %v{}, %v{}", dst, lhs.0, rhs.0)?
                    }
                    Instr::Mul { dst, lhs, rhs } => {
                        writeln!(f, "%v{} = mul %v{}, %v{}", dst, lhs.0, rhs.0)?
                    }
                    Instr::Div { dst, lhs, rhs } => {
                        writeln!(f, "%v{} = div %v{}, %v{}", dst, lhs.0, rhs.0)?
                    }
                    Instr::Eq { dst, lhs, rhs } => {
                        writeln!(f, "%v{} = eq %v{}, %v{}", dst, lhs.0, rhs.0)?
                    }
                    Instr::Lt { dst, lhs, rhs } => {
                        writeln!(f, "%v{} = lt %v{}, %v{}", dst, lhs.0, rhs.0)?
                    }
                    Instr::Gt { dst, lhs, rhs } => {
                        writeln!(f, "%v{} = gt %v{}, %v{}", dst, lhs.0, rhs.0)?
                    }
                    Instr::LoadConst { dst, value } => writeln!(f, "%v{} = {}", dst, value)?,
                    Instr::Call { dst, func, args } => {
                        write!(f, "%v{} = ", dst.0)?;
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
                    Instr::Cast { value, from } => {
                        let t = value;
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
                    Terminator::Branch { cond, yes, no } => {
                        writeln!(f, "br %v{}, b{}, b{}", cond.0, yes.0, no.0)?
                    }
                }
            }
        }

        writeln!(f, "}}")
    }
}

#[cfg(test)]
mod ir {
    #[test]
    fn print_ir_example() {
        use crate::ir::*;

        let v0 = TypeId {
            ty: Type::Int,
            id: Id(0),
        };
        let v1 = TypeId {
            ty: Type::Int,
            id: Id(1),
        };
        let v2 = TypeId {
            ty: Type::Int,
            id: Id(2),
        };
        let v3 = TypeId {
            ty: Type::Int,
            id: Id(3),
        };
        let v4 = TypeId {
            ty: Type::Int,
            id: Id(4),
        };

        let b0 = Id(0);

        let block0 = Block {
            id: b0,
            tombstone: false,
            params: vec![v0.clone(), v1.clone(), v2.clone()],
            instructions: vec![
                Instr::Add {
                    dst: v3.clone(),
                    lhs: v1.id,
                    rhs: v2.id,
                },
                Instr::Add {
                    dst: v4.clone(),
                    lhs: v0.id,
                    rhs: v3.id,
                },
            ],
            term: Some(Terminator::Return(Some(v4.id))),
        };

        let func = Func {
            id: Id(0),
            name: "test",
            ret: Some(Type::Int),
            blocks: vec![block0],
        };

        println!("{}", func);
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
