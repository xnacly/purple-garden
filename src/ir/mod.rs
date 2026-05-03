//! The purple garden immediate representation, it aims to have/be:
//!
//! - Explicit data flow
//! - No hidden control flow
//! - No implicit state mutation (its pure)
//! - Stable semantics under rewriting
//! - Cheap to analyze
//!
//! The immediate representation sits between the AST the parser produces and the virtual machine
//! specific bytecode / machine code produced by the JIT, either x86-64 or aarch64. It allows for
//! optimisations, like:
//!
//! - constant folding/propagation
//! - common subexpression elimination
//! - copy propagations
//! - dead code elimination
//! - algebraic simplification
//! - inlining
//! - tail call optimisation
//! - jump threading

mod display;
pub mod lower;
pub mod ptype;
pub mod typecheck;

use std::collections::HashMap;

use crate::ir::ptype::Type;
use crate::std as pstd;

/// Compile time Value representation, used for interning and constant propagation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, Default)]
pub enum Const<'c> {
    #[default]
    Undefined,
    False,
    True,
    Int(i64),
    Double(u64),
    Str(&'c str),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(pub u32);

#[derive(Debug, Clone)]
pub struct TypeId {
    pub id: Id,
    pub ty: Type,
}

#[derive(Debug, Clone)]
pub enum BinOp {
    IAdd,
    ISub,
    IMul,
    IDiv,
    ILt,
    IGt,
    IEq,
    DAdd,
    DSub,
    DMul,
    DDiv,
    DLt,
    DGt,
    BEq,
}

#[derive(Debug, Clone)]
pub enum Instr<'i> {
    Bin {
        op: BinOp,
        dst: TypeId,
        lhs: Id,
        rhs: Id,
    },
    LoadConst {
        dst: TypeId,
        value: Const<'i>,
    },
    Call {
        dst: TypeId,
        func: Id,
        args: Vec<Id>,
    },
    Sys {
        dst: TypeId,
        path: &'i str,
        func: &'i pstd::Fn,
        args: Vec<Id>,
    },
    Tail {
        dst: TypeId,
        func: Id,
        args: Vec<Id>,
    },
    Cast {
        dst: TypeId,
        from: Id,
    },
    Noop,
}

#[derive(Debug, Clone)]
pub enum Terminator {
    Return(Option<Id>),
    Jump {
        id: Id,
        params: Vec<Id>,
    },
    Branch {
        cond: Id,
        yes: (Id, Vec<Id>),
        no: (Id, Vec<Id>),
    },
}

#[derive(Debug, Clone)]
pub struct Block<'b> {
    /// block is dead as a result of optimisation passes
    pub tombstone: bool,
    pub id: Id,
    pub instructions: Vec<Instr<'b>>,
    pub params: Vec<Id>,
    /// each block has a term, but a block starts without one in the lowering process, thus this
    /// field has to be optional
    pub term: Option<Terminator>,
}

#[derive(Debug, Clone, Default)]
pub struct Func<'f> {
    pub name: &'f str,
    pub id: Id,
    pub params: Vec<Id>,
    pub ret: Option<Type>,
    pub blocks: Vec<Block<'f>>,
}

impl Func<'_> {
    /// Map VReg to its (start, end)
    pub fn live_set(&self) -> HashMap<Id, (Id, Id)> {
        let mut ranges: HashMap<Id, (Id, Id)> = HashMap::new();
        let mut block_pos: HashMap<Id, u32> = HashMap::new();
        let mut pos = 0u32;

        for block in &self.blocks {
            if block.tombstone {
                continue;
            }
            block_pos.insert(block.id, pos);
            pos += block.instructions.len() as u32;
            if block.term.is_some() {
                pos += 1;
            }
        }

        fn define(ranges: &mut HashMap<Id, (Id, Id)>, id: Id, pos: u32) {
            ranges.entry(id).or_insert((Id(pos), Id(pos)));
        }

        fn use_id(ranges: &mut HashMap<Id, (Id, Id)>, id: Id, pos: u32) {
            let entry = ranges.entry(id).or_insert((Id(pos), Id(pos)));
            entry.1 = Id(entry.1.0.max(pos));
        }

        fn use_edge_params(
            ranges: &mut HashMap<Id, (Id, Id)>,
            block_pos: &HashMap<Id, u32>,
            edge_pos: u32,
            target: Id,
            params: &[Id],
        ) {
            let Some(target_pos) = block_pos.get(&target).copied() else {
                return;
            };
            let use_pos = edge_pos.max(target_pos);

            for param in params {
                use_id(ranges, *param, use_pos);
            }
        }

        for block in &self.blocks {
            if block.tombstone {
                continue;
            }

            let mut pos = *block_pos
                .get(&block.id)
                .expect("non-tombstone block must have a position");

            for param in &block.params {
                define(&mut ranges, *param, pos);
            }

            for instr in &block.instructions {
                match instr {
                    Instr::Bin { dst, lhs, rhs, .. } => {
                        use_id(&mut ranges, *lhs, pos);
                        use_id(&mut ranges, *rhs, pos);
                        define(&mut ranges, dst.id, pos);
                    }
                    Instr::LoadConst { dst, .. } => define(&mut ranges, dst.id, pos),
                    Instr::Call { dst, args, .. }
                    | Instr::Sys { dst, args, .. }
                    | Instr::Tail { dst, args, .. } => {
                        for arg in args {
                            use_id(&mut ranges, *arg, pos);
                        }
                        define(&mut ranges, dst.id, pos);
                    }
                    Instr::Cast { dst, from } => {
                        use_id(&mut ranges, *from, pos);
                        define(&mut ranges, dst.id, pos);
                    }
                    Instr::Noop => {}
                }
                pos += 1;
            }

            if let Some(term) = &block.term {
                match term {
                    Terminator::Return(Some(id)) => use_id(&mut ranges, *id, pos),
                    Terminator::Return(None) => {}
                    Terminator::Jump { id, params } => {
                        use_edge_params(&mut ranges, &block_pos, pos, *id, params);
                    }
                    Terminator::Branch {
                        cond,
                        yes: (yes, yes_params),
                        no: (no, no_params),
                    } => {
                        use_id(&mut ranges, *cond, pos);
                        use_edge_params(&mut ranges, &block_pos, pos, *yes, yes_params);
                        use_edge_params(&mut ranges, &block_pos, pos, *no, no_params);
                    }
                }
            }
        }

        ranges
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int_id(id: u32) -> TypeId {
        TypeId {
            id: Id(id),
            ty: Type::Int,
        }
    }

    #[test]
    fn live_set_extends_values_passed_to_block_params() {
        let func = Func {
            name: "test",
            id: Id(0),
            params: vec![],
            ret: None,
            blocks: vec![
                Block {
                    tombstone: false,
                    id: Id(0),
                    instructions: vec![Instr::LoadConst {
                        dst: int_id(0),
                        value: Const::Int(1),
                    }],
                    params: vec![],
                    term: Some(Terminator::Jump {
                        id: Id(1),
                        params: vec![Id(0)],
                    }),
                },
                Block {
                    tombstone: false,
                    id: Id(1),
                    instructions: vec![Instr::Bin {
                        op: BinOp::IAdd,
                        dst: int_id(2),
                        lhs: Id(1),
                        rhs: Id(1),
                    }],
                    params: vec![Id(1)],
                    term: Some(Terminator::Return(Some(Id(2)))),
                },
            ],
        };

        let live = func.live_set();
        assert_eq!(live.get(&Id(0)), Some(&(Id(0), Id(2))));
        assert_eq!(live.get(&Id(1)), Some(&(Id(2), Id(2))));
    }
}
