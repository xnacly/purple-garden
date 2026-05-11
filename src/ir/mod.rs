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

    /// Per-SSA live interval, indexed by id. `(u32::MAX, 0)` marks a slot
    /// with no def — only happens for params of tombstoned blocks since
    /// SSA ids are otherwise dense.
    pub fn live_set(&self) -> Vec<(u32, u32)> {
        const UNSET: (u32, u32) = (u32::MAX, 0);

        fn ensure(v: &mut Vec<(u32, u32)>, id: u32) {
            let idx = id as usize;
            if idx >= v.len() {
                v.resize(idx + 1, UNSET);
            }
        }

        fn define(intervals: &mut Vec<(u32, u32)>, id: Id, pos: u32) {
            ensure(intervals, id.0);
            let e = &mut intervals[id.0 as usize];
            if e.0 == u32::MAX {
                *e = (pos, pos);
            } else {
                e.0 = e.0.min(pos);
                e.1 = e.1.max(pos);
            }
        }

        fn use_value(intervals: &mut Vec<(u32, u32)>, id: Id, pos: u32) {
            ensure(intervals, id.0);
            let e = &mut intervals[id.0 as usize];
            if e.0 == u32::MAX {
                *e = (pos, pos);
            } else {
                e.1 = e.1.max(pos);
            }
        }

        crate::trace!("[ir::Func::live_set][{}] start", self.name);

        let mut intervals: Vec<(u32, u32)> = Vec::new();
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
                crate::trace!(
                    "[ir::Func::live_set] def %v{} @{} (b{} param)",
                    param.0,
                    pos,
                    block.id.0
                );
                define(&mut intervals, *param, pos);
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
                    crate::trace!(
                        "[ir::Func::live_set] use %v{} @{} (b{} instr {})",
                        use_id.0,
                        pos,
                        block.id.0,
                        instr
                    );
                    use_value(&mut intervals, use_id, pos);
                }

                if let Some(def_id) = Self::def_of(instr) {
                    crate::trace!(
                        "[ir::Func::live_set] def %v{} @{} (b{} instr {})",
                        def_id.0,
                        pos,
                        block.id.0,
                        instr
                    );
                    define(&mut intervals, def_id, pos);
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
                    crate::trace!(
                        "[ir::Func::live_set] use %v{} @{} (b{} term {})",
                        use_id.0,
                        pos,
                        block.id.0,
                        term
                    );
                    use_value(&mut intervals, use_id, pos);
                }

                // The bc emitter writes outgoing param values into the
                // successor's param registers right before the terminator
                // op. For a Branch this means the yes-target shuffle can
                // clobber cond before JmpT reads it (cond appears dead to
                // the regalloc at the terminator, so its register is
                // reusable as a shuffle dst). Marking the successor params
                // as defined here forces the regalloc to keep them out of
                // cond's register.
                //
                // TODO: a parallel-move resolver in the bc emitter would
                // be the more general fix — it would also close the
                // analogous parallel-move hazards on Jump/Tail (where
                // dst[i] == src[j] for j > i clobbers a not-yet-read
                // source).
                if let Terminator::Branch { yes, no, .. } = term {
                    for &target_param in &self.blocks[yes.0.0 as usize].params {
                        crate::trace!(
                            "[ir::Func::live_set] def %v{} @{} (b{} branch yes shuffle dst)",
                            target_param.0,
                            pos,
                            block.id.0
                        );
                        define(&mut intervals, target_param, pos);
                    }
                    for &target_param in &self.blocks[no.0.0 as usize].params {
                        crate::trace!(
                            "[ir::Func::live_set] def %v{} @{} (b{} branch no shuffle dst)",
                            target_param.0,
                            pos,
                            block.id.0
                        );
                        define(&mut intervals, target_param, pos);
                    }
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
        for (id, &(def, last_use)) in intervals.iter().enumerate() {
            if def == u32::MAX {
                continue;
            }
            crate::trace!(
                "[ir::Func::live_set][{}] interval %v{} = ({}..{})",
                self.name,
                id,
                def,
                last_use
            );
        }

        intervals
    }

    /// Per-SSA register hints for `Ralloc`. Walks every call-shaped
    /// instruction/terminator in this function and records that:
    /// - `args[i]` would prefer `r_i` (the arg-passing register)
    /// - the result of a `Call`/`Sys` would prefer `r0` (the return
    ///   register, so the post-call `Mov dst, r0` becomes a self-mov that
    ///   peephole + `compact_nops` will erase).
    ///
    /// Soft: the allocator honors the hint only if the preferred register
    /// is free when this interval is allocated. If multiple call sites
    /// hint the same SSA id to different registers, first hint wins.
    pub fn arg_hints(&self) -> Vec<Option<u8>> {
        let mut hints: Vec<Option<u8>> = Vec::new();

        fn ensure(v: &mut Vec<Option<u8>>, id: u32) {
            let idx = id as usize;
            if idx >= v.len() {
                v.resize(idx + 1, None);
            }
        }
        // First-hint-wins: skip if already set.
        fn put(v: &mut Vec<Option<u8>>, id: Id, reg: u8) {
            ensure(v, id.0);
            let e = &mut v[id.0 as usize];
            if e.is_none() {
                *e = Some(reg);
            }
        }
        // Overwrite unconditionally — used for entry-block params so they
        // beat any subsequent inner-call hint and stay pinned to the
        // calling convention's r0..r{N-1}.
        fn put_force(v: &mut Vec<Option<u8>>, id: Id, reg: u8) {
            ensure(v, id.0);
            v[id.0 as usize] = Some(reg);
        }

        // Entry block params arrive in r0..r{N-1} per the calling convention.
        // Pin them first so they take priority over inner-call hints —
        // otherwise an inner call that uses the function's first param as
        // its arg-2 would hint it to r2, the regalloc would place it in
        // r2, and the caller still writes the arg to r0 → the function
        // reads garbage.
        if let Some(entry) = self.blocks.first()
            && !entry.tombstone
        {
            for (i, param) in entry.params.iter().enumerate() {
                put_force(&mut hints, *param, i as u8);
            }
        }

        for block in &self.blocks {
            if block.tombstone {
                continue;
            }
            for instr in &block.instructions {
                match instr {
                    Instr::Call { dst, args, .. } | Instr::Sys { dst, args, .. } => {
                        for (i, arg) in args.iter().enumerate() {
                            put(&mut hints, *arg, i as u8);
                        }
                        put(&mut hints, dst.id, 0u8);
                    }
                    _ => {}
                }
            }
            if let Some(Terminator::Tail { args, .. }) = &block.term {
                for (i, arg) in args.iter().enumerate() {
                    put(&mut hints, *arg, i as u8);
                }
            }
        }
        hints
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

        assert_eq!(live_set[0], (0, 3));
        assert_eq!(live_set[1], (1, 3));
        assert_eq!(live_set[2], (2, 3));
        // %v3 and %v4 are successor block params of the Branch in block 0
        // and so are extra-defined at the branch position (pos=3) — see
        // live_set: this is what keeps them out of cond's register at the
        // shuffle.
        assert_eq!(live_set[3], (3, 5));
        assert_eq!(live_set[4], (3, 7));
        assert_eq!(live_set[5], (7, 8));
    }
}
