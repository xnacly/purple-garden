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

use std::collections::{HashMap, HashSet};

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
    Tail {
        func: Id,
        args: Vec<Id>,
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

#[derive(Debug, Clone)]
struct Edge {
    to: Id,
    args: Vec<Id>,
}

impl Func<'_> {
    fn def_of(instr: &Instr<'_>) -> Option<Id> {
        match instr {
            Instr::Bin { dst, .. }
            | Instr::LoadConst { dst, .. }
            | Instr::Call { dst, .. }
            | Instr::Sys { dst, .. }
            | Instr::Cast { dst, .. } => Some(dst.id),
            Instr::Noop => None,
        }
    }

    fn uses_of_instr(instr: &Instr<'_>) -> Vec<Id> {
        match instr {
            Instr::Bin { lhs, rhs, .. } => vec![*lhs, *rhs],
            Instr::Call { args, .. } | Instr::Sys { args, .. } => args.clone(),
            Instr::Cast { from, .. } => vec![*from],
            Instr::LoadConst { .. } | Instr::Noop => vec![],
        }
    }

    fn uses_of_term(term: &Terminator) -> Vec<Id> {
        match term {
            Terminator::Return(Some(id)) => vec![*id],
            Terminator::Return(None) => vec![],
            Terminator::Jump { params, .. } => params.clone(),
            Terminator::Tail { args, .. } => args.clone(),
            Terminator::Branch {
                cond,
                yes: (_, yes_params),
                no: (_, no_params),
            } => {
                let mut uses = Vec::with_capacity(1 + yes_params.len() + no_params.len());
                uses.push(*cond);
                uses.extend(yes_params.iter().copied());
                uses.extend(no_params.iter().copied());
                uses
            }
        }
    }

    fn local_term_uses(term: &Terminator) -> Vec<Id> {
        match term {
            Terminator::Return(Some(id)) => vec![*id],
            Terminator::Return(None) | Terminator::Jump { .. } => vec![],
            Terminator::Tail { args, .. } => args.clone(),
            Terminator::Branch { cond, .. } => vec![*cond],
        }
    }

    fn successors(term: Option<&Terminator>) -> Vec<Edge> {
        match term {
            Some(Terminator::Jump { id, params }) => vec![Edge {
                to: *id,
                args: params.clone(),
            }],
            Some(Terminator::Branch { yes, no, .. }) => vec![
                Edge {
                    to: yes.0,
                    args: yes.1.clone(),
                },
                Edge {
                    to: no.0,
                    args: no.1.clone(),
                },
            ],
            Some(Terminator::Return(_) | Terminator::Tail { .. }) | None => vec![],
        }
    }

    pub fn live_set(&self) -> HashMap<Id, (Id, Id)> {
        // PERF: this whole process should be a set theory based bit set, since we have at most 64
        // registers (i think?), thus a BitSet(u64) should be a perfect abstraction

        fn define(intervals: &mut HashMap<Id, (Id, Id)>, id: Id, pos: u32, reason: String) {
            crate::trace!("[ir::Func::live_set] def %v{} @{} ({})", id.0, pos, reason);
            intervals
                .entry(id)
                .and_modify(|(def, last_use)| {
                    def.0 = def.0.min(pos);
                    last_use.0 = last_use.0.max(pos);
                })
                .or_insert((Id(pos), Id(pos)));
        }

        fn use_value(intervals: &mut HashMap<Id, (Id, Id)>, id: Id, pos: u32, reason: String) {
            crate::trace!("[ir::Func::live_set] use %v{} @{} ({})", id.0, pos, reason);
            intervals
                .entry(id)
                .and_modify(|(_, last_use)| last_use.0 = last_use.0.max(pos))
                .or_insert((Id(pos), Id(pos)));
        }

        crate::trace!("[ir::Func::live_set][{}] start", self.name);

        let mut intervals = HashMap::new();
        let mut pos = 0;

        for block in &self.blocks {
            if block.tombstone {
                crate::trace!(
                    "[ir::Func::live_set][{}] skip tombstone b{}",
                    self.name,
                    block.id.0
                );
                continue;
            }

            crate::trace!(
                "[ir::Func::live_set][{}] b{} entry @{}",
                self.name,
                block.id.0,
                pos
            );
            for param in &block.params {
                define(
                    &mut intervals,
                    *param,
                    pos,
                    format!("b{} param", block.id.0),
                );
            }
            pos += 1;

            for instr in &block.instructions {
                crate::trace!(
                    "[ir::Func::live_set][{}] b{} instr @{}: {}",
                    self.name,
                    block.id.0,
                    pos,
                    instr
                );

                for use_id in Self::uses_of_instr(instr) {
                    use_value(
                        &mut intervals,
                        use_id,
                        pos,
                        format!("b{} instr {}", block.id.0, instr),
                    );
                }

                if let Some(def_id) = Self::def_of(instr) {
                    define(
                        &mut intervals,
                        def_id,
                        pos,
                        format!("b{} instr {}", block.id.0, instr),
                    );
                }
                pos += 1;
            }

            if let Some(term) = &block.term {
                crate::trace!(
                    "[ir::Func::live_set][{}] b{} term @{}: {}",
                    self.name,
                    block.id.0,
                    pos,
                    term
                );

                for use_id in Self::uses_of_term(term) {
                    use_value(
                        &mut intervals,
                        use_id,
                        pos,
                        format!("b{} term {}", block.id.0, term),
                    );
                }
            } else {
                crate::trace!(
                    "[ir::Func::live_set][{}] b{} has no terminator @{}",
                    self.name,
                    block.id.0,
                    pos
                );
            }
            pos += 1;
        }

        #[cfg(feature = "trace")]
        {
            let mut ordered: Vec<_> = intervals.iter().collect();
            ordered.sort_by_key(|(id, _)| id.0);
            for (id, (def, last_use)) in ordered {
                crate::trace!(
                    "[ir::Func::live_set][{}] interval %v{} = ({}..{})",
                    self.name,
                    id.0,
                    def.0,
                    last_use.0
                );
            }
        }

        intervals
    }
}

#[cfg(test)]
mod tests {
    use super::{ptype::Type, *};
    use std::collections::HashSet;

    fn type_id(id: u32) -> TypeId {
        TypeId {
            id: Id(id),
            ty: Type::Int,
        }
    }

    fn ids(ids: &[u32]) -> HashSet<Id> {
        ids.iter().copied().map(Id).collect()
    }

    fn empty_block(id: u32, params: Vec<Id>, term: Terminator) -> Block<'static> {
        Block {
            tombstone: false,
            id: Id(id),
            params,
            instructions: vec![],
            term: Some(term),
        }
    }

    #[test]
    fn live_set_tracks_block_params_instructions_and_terminators() {
        let fun = Func {
            name: "live",
            id: Id(0),
            params: vec![Id(0)],
            ret: Some(Type::Int),
            blocks: vec![
                Block {
                    tombstone: false,
                    id: Id(0),
                    params: vec![Id(0)],
                    instructions: vec![
                        Instr::LoadConst {
                            dst: type_id(1),
                            value: Const::Int(1),
                        },
                        Instr::Bin {
                            op: BinOp::IAdd,
                            dst: type_id(2),
                            lhs: Id(0),
                            rhs: Id(1),
                        },
                    ],
                    term: Some(Terminator::Branch {
                        cond: Id(2),
                        yes: (Id(1), vec![Id(0)]),
                        no: (Id(2), vec![Id(1)]),
                    }),
                },
                Block {
                    tombstone: false,
                    id: Id(1),
                    params: vec![Id(3)],
                    instructions: vec![],
                    term: Some(Terminator::Return(Some(Id(3)))),
                },
                Block {
                    tombstone: false,
                    id: Id(2),
                    params: vec![Id(4)],
                    instructions: vec![Instr::Cast {
                        dst: type_id(5),
                        from: Id(4),
                    }],
                    term: Some(Terminator::Return(Some(Id(5)))),
                },
            ],
        };

        let live_set = fun.live_set();

        assert_eq!(live_set.get(&Id(0)), Some(&(Id(0), Id(3))));
        assert_eq!(live_set.get(&Id(1)), Some(&(Id(1), Id(3))));
        assert_eq!(live_set.get(&Id(2)), Some(&(Id(2), Id(3))));
        assert_eq!(live_set.get(&Id(3)), Some(&(Id(4), Id(5))));
        assert_eq!(live_set.get(&Id(4)), Some(&(Id(6), Id(7))));
        assert_eq!(live_set.get(&Id(5)), Some(&(Id(7), Id(8))));
    }
}
