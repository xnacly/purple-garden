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

use crate::ir::ptype::Type;

/// Compile time Value representation, used for interning and constant propagation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub enum Const<'c> {
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
pub enum Instr<'i> {
    Add { dst: TypeId, lhs: Id, rhs: Id },
    Sub { dst: TypeId, lhs: Id, rhs: Id },
    Mul { dst: TypeId, lhs: Id, rhs: Id },
    Div { dst: TypeId, lhs: Id, rhs: Id },
    Eq { dst: TypeId, lhs: Id, rhs: Id },
    Lt { dst: TypeId, lhs: Id, rhs: Id },
    Gt { dst: TypeId, lhs: Id, rhs: Id },
    LoadConst { dst: TypeId, value: Const<'i> },
    Call { dst: Id, func: Id, args: Vec<Id> },
    Cast { value: TypeId, from: Id },
    Noop,
}

#[derive(Debug, Clone)]
pub enum Terminator {
    Return(Option<Id>),
    Jump { id: Id, params: Vec<Id> },
    // TODO: this needs params
    Branch { cond: Id, yes: Id, no: Id },
}

#[derive(Debug, Clone)]
pub struct Block<'b> {
    /// block is dead as a result of optimisation passes
    pub tombstone: bool,
    pub id: Id,
    pub instructions: Vec<Instr<'b>>,
    pub params: Vec<TypeId>,
    /// each block has a term, but a block starts without one in the lowering process, thus this
    /// field has to be optional
    pub term: Option<Terminator>,
}

#[derive(Debug, Clone, Default)]
pub struct Func<'f> {
    pub name: &'f str,
    pub id: Id,
    pub ret: Option<Type>,
    pub blocks: Vec<Block<'f>>,
}
