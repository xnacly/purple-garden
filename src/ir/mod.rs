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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
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
    // TODO: rework this to work in reverse, last use and first def should be easier to compute,
    // since the value id is the definition and the first seen usage is the last usage

    /// mapping any vN to (def, last_use), used for spill detection for preserving registers around
    /// call boundaries, since all registers in pg are callersaved (def(v) lteq C lt last_use(v)).
    pub fn live_set(&self) -> HashMap<u32, (u32, u32)> {
        let mut def = HashMap::new();
        let mut last_use = HashMap::new();
        let mut live_set = HashMap::new();

        let mut idx = 0;
        for param in &self.params {
            def.insert(param.0, idx);
            last_use.entry(param.0).or_insert(idx);
        }

        for block in &self.blocks {
            for instr in &block.instructions {
                match instr {
                    Instr::Bin { dst, lhs, rhs, .. } => {
                        last_use.insert(lhs.0, idx);
                        last_use.insert(rhs.0, idx);

                        def.insert(dst.id.0, idx);
                        last_use.entry(dst.id.0).or_insert(idx);
                    }
                    Instr::LoadConst { dst, .. } => {
                        def.insert(dst.id.0, idx);
                        last_use.entry(dst.id.0).or_insert(idx);
                    }
                    Instr::Call { dst, args, .. }
                    | Instr::Sys { dst, args, .. }
                    | Instr::Tail { dst, args, .. } => {
                        for arg in args {
                            last_use.insert(arg.0, idx);
                        }

                        def.insert(dst.id.0, idx);
                        last_use.entry(dst.id.0).or_insert(idx);
                    }
                    Instr::Cast { dst, from } => {
                        last_use.insert(from.0, idx);

                        def.insert(dst.id.0, idx);
                        last_use.entry(dst.id.0).or_insert(idx);
                    }
                    _ => unreachable!(),
                }

                idx += 1;
            }

            if let Some(term) = &block.term {
                match term {
                    Terminator::Return(Some(Id(id))) => {
                        last_use.insert(*id, idx);
                    }
                    Terminator::Jump { params, .. } => {
                        for id in params {
                            last_use.insert(id.0, idx);
                        }
                    }
                    Terminator::Branch { cond, yes, no } => {
                        last_use.insert(cond.0, idx);

                        for id in &yes.1 {
                            last_use.insert(id.0, idx);
                        }

                        for id in &no.1 {
                            last_use.insert(id.0, idx);
                        }
                    }
                    _ => (),
                }
            }
        }

        for (v, d) in def {
            let l = last_use.get(&v).copied().unwrap_or(d);
            live_set.insert(v, (d, l));
        }

        live_set
    }
}
