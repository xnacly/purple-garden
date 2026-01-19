//! The purple garden immediate representation, it aims to have/be:
//!
//! - Explicit data flow
//!
//! - No hidden control flow
//!
//! - No implicit state mutation
//!
//! - Stable semantics under rewriting
//!
//! - Cheap to analyze

use std::fmt::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Id(u32);

pub enum Instr {
    Add {
        dst: Id,
        lhs: Id,
        rhs: Id,
    },
    Sub {
        dst: Id,
        lhs: Id,
        rhs: Id,
    },
    Mul {
        dst: Id,
        lhs: Id,
        rhs: Id,
    },
    Div {
        dst: Id,
        lhs: Id,
        rhs: Id,
    },

    LoadConst {
        dst: Id,
        // TODO: crate::cc::Const needs to be ripped out from cc into ir
        value: crate::cc::Const<'static>,
    },

    Call {
        dst: Option<Id>,
        func: Id,
        args: Vec<Id>,
    },
}

pub enum Terminator {
    Return(Option<Id>),
    Jump { id: Id, params: Vec<Id> },
    Branch { cond: Id, yes: Id, no: Id },
}

pub struct Block {
    id: Id,
    instructions: Vec<Instr>,
    params: Vec<Id>,
    term: Terminator,
}

pub struct Func {
    id: Id,
    entry: Id,
    blocks: Vec<Block>,
}

impl Display for Func {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let entry_block = self
            .blocks
            .iter()
            .find(|b| b.id == self.entry)
            .expect("Func.entry does not reference a valid block");

        write!(f, "fn @f{}(", self.id.0)?;
        for (i, arg) in entry_block.params.iter().enumerate() {
            if i + 1 == entry_block.params.len() {
                write!(f, "%v{}", arg.0)?;
            } else {
                write!(f, "%v{}, ", arg.0)?;
            }
        }
        writeln!(f, ") {{")?;

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
                        .map(|p| format!("%v{}", p.0))
                        .collect::<Vec<_>>()
                        .join(", ")
                )?;
            }

            for ins in &block.instructions {
                match ins {
                    Instr::Add { dst, lhs, rhs } => {
                        writeln!(f, "%v{} = add %v{}, %v{}", dst.0, lhs.0, rhs.0)?
                    }
                    Instr::Sub { dst, lhs, rhs } => {
                        writeln!(f, "%v{} = sub %v{}, %v{}", dst.0, lhs.0, rhs.0)?
                    }
                    Instr::Mul { dst, lhs, rhs } => {
                        writeln!(f, "%v{} = mul %v{}, %v{}", dst.0, lhs.0, rhs.0)?
                    }
                    Instr::Div { dst, lhs, rhs } => {
                        writeln!(f, "%v{} = div %v{}, %v{}", dst.0, lhs.0, rhs.0)?
                    }
                    Instr::LoadConst { dst, value } => writeln!(f, "%v{} = {:?}", dst.0, value)?,
                    Instr::Call { dst, func, args } => {
                        if let Some(dst) = dst {
                            write!(f, "%v{} = ", dst.0)?;
                        }
                        write!(f, "@f{}(", func.0)?;
                        for (i, arg) in args.iter().enumerate() {
                            if i + 1 == args.len() {
                                write!(f, "%v{}", arg.0)?;
                            } else {
                                write!(f, "%v{}, ", arg.0)?;
                            }
                        }
                        writeln!(f, ")")?;
                    }
                }
            }

            match &block.term {
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

        writeln!(f, "}}")
    }
}

#[cfg(test)]
mod ir {
    #[test]
    fn print_ir_example() {
        use crate::ir::*;

        let v0 = Id(0);
        let v1 = Id(1);
        let v2 = Id(2);
        let v3 = Id(3);
        let v4 = Id(4);

        let b0 = Id(0);

        let block0 = Block {
            id: b0,
            params: vec![v0, v1, v2],
            instructions: vec![
                Instr::Add {
                    dst: v3,
                    lhs: v1,
                    rhs: v2,
                },
                Instr::Add {
                    dst: v4,
                    lhs: v0,
                    rhs: v3,
                },
            ],
            term: Terminator::Return(Some(v4)),
        };

        let func = Func {
            id: Id(0),
            entry: block0.id,
            blocks: vec![block0],
        };

        println!("{}", func);
    }
}
