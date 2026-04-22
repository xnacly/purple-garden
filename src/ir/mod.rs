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
    /// Map VReg to its (start, end)
    pub fn live_set(&self) -> HashMap<Id, (Id, Id)> {
        struct Liveness {
            /// LiveIn[B]: set of VRegs alive at B entry
            live_in: HashMap<Id, Vec<Id>>,
            /// LiveOut[B]: set of VRegs alive at B exit
            live_out: HashMap<Id, Vec<Id>>,
        }

        for b in &self.blocks {
            let live_in = &b.params;
            let live_out = &b.term;
            dbg!((live_in, live_out));
        }

        HashMap::new()
    }
}
