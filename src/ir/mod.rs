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
        /// Byte offset into the source of the originating AST node. Threaded
        /// through to `bc::Cc::pc_to_span` so runtime traps render with the
        /// right `file:line:col`. See `Vm::pc_to_span`.
        span: u32,
    },
    LoadConst {
        dst: TypeId,
        value: Const<'i>,
        span: u32,
    },
    Call {
        dst: TypeId,
        func: Id,
        args: Vec<Id>,
        span: u32,
    },
    Sys {
        dst: TypeId,
        path: &'i str,
        func: &'i pstd::Fn,
        args: Vec<Id>,
        span: u32,
    },
    Cast {
        dst: TypeId,
        from: TypeId,
        span: u32,
    },
    Noop,
}

impl Instr<'_> {
    /// Source byte offset for this instruction, or 0 for synthetic Noops
    /// (which never trap, so the missing span doesn't matter).
    pub fn span(&self) -> u32 {
        match self {
            Instr::Bin { span, .. }
            | Instr::LoadConst { span, .. }
            | Instr::Call { span, .. }
            | Instr::Sys { span, .. }
            | Instr::Cast { span, .. } => *span,
            Instr::Noop => 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Terminator {
    Return {
        value: Option<Id>,
        span: u32,
    },
    Jump {
        id: Id,
        params: Vec<Id>,
        span: u32,
    },
    Branch {
        cond: Id,
        yes: (Id, Vec<Id>),
        no: (Id, Vec<Id>),
        span: u32,
    },
    Tail {
        func: Id,
        args: Vec<Id>,
        span: u32,
    },
}

impl Terminator {
    pub fn span(&self) -> u32 {
        match self {
            Terminator::Return { span, .. }
            | Terminator::Jump { span, .. }
            | Terminator::Branch { span, .. }
            | Terminator::Tail { span, .. } => *span,
        }
    }
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
            Instr::Cast { from, .. } => vec![from.id],
            Instr::LoadConst { .. } | Instr::Noop => vec![],
        }
    }

    fn uses_of_term(term: &Terminator) -> Vec<Id> {
        match term {
            Terminator::Return { value: Some(id), .. } => vec![*id],
            Terminator::Return { value: None, .. } => vec![],
            Terminator::Jump { params, .. } => params.clone(),
            Terminator::Tail { args, .. } => args.clone(),
            Terminator::Branch {
                cond,
                yes: (_, yes_params),
                no: (_, no_params),
                ..
            } => {
                let mut uses = Vec::with_capacity(1 + yes_params.len() + no_params.len());
                uses.push(*cond);
                uses.extend(yes_params.iter().copied());
                uses.extend(no_params.iter().copied());
                uses
            }
        }
    }

    /// Per-SSA live interval, indexed by id. `(u32::MAX, 0)` marks a slot
    /// with no def — only happens for params of tombstoned blocks since
    /// SSA ids are otherwise dense.
    ///
    /// Writes into `out`, clearing first. Lets the caller reuse a buffer
    /// across function compiles so we don't allocate fresh per `cc()`.
    pub fn live_set_into(&self, out: &mut Vec<(u32, u32)>) {
        const UNSET: (u32, u32) = (u32::MAX, 0);
        out.clear();

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

        let intervals = &mut *out;
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
                define(intervals, *param, pos);
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
                    use_value(intervals, use_id, pos);
                }

                if let Some(def_id) = Self::def_of(instr) {
                    crate::trace!(
                        "[ir::Func::live_set] def %v{} @{} (b{} instr {})",
                        def_id.0,
                        pos,
                        block.id.0,
                        instr
                    );
                    define(intervals, def_id, pos);
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
                    use_value(intervals, use_id, pos);
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
                        define(intervals, target_param, pos);
                    }
                    for &target_param in &self.blocks[no.0.0 as usize].params {
                        crate::trace!(
                            "[ir::Func::live_set] def %v{} @{} (b{} branch no shuffle dst)",
                            target_param.0,
                            pos,
                            block.id.0
                        );
                        define(intervals, target_param, pos);
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
    pub fn arg_hints_into(&self, hints: &mut Vec<Option<u8>>) {
        hints.clear();

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
                put_force(hints, *param, i as u8);
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
                            put(hints, *arg, i as u8);
                        }
                        put(hints, dst.id, 0u8);
                    }
                    _ => {}
                }
            }
            if let Some(Terminator::Tail { args, .. }) = &block.term {
                for (i, arg) in args.iter().enumerate() {
                    put(hints, *arg, i as u8);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ptype::Type, *};

    fn type_id(id: u32) -> TypeId {
        TypeId {
            id: Id(id),
            ty: Type::Int,
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
                            span: 0,
                        },
                        Instr::Bin {
                            op: BinOp::IAdd,
                            dst: type_id(2),
                            lhs: Id(0),
                            rhs: Id(1),
                            span: 0,
                        },
                    ],
                    term: Some(Terminator::Branch {
                        cond: Id(2),
                        yes: (Id(1), vec![Id(0)]),
                        no: (Id(2), vec![Id(1)]),
                        span: 0,
                    }),
                },
                Block {
                    tombstone: false,
                    id: Id(1),
                    params: vec![Id(3)],
                    instructions: vec![],
                    term: Some(Terminator::Return {
                        value: Some(Id(3)),
                        span: 0,
                    }),
                },
                Block {
                    tombstone: false,
                    id: Id(2),
                    params: vec![Id(4)],
                    instructions: vec![Instr::Cast {
                        dst: type_id(5),
                        from: type_id(4),
                        span: 0,
                    }],
                    term: Some(Terminator::Return {
                        value: Some(Id(5)),
                        span: 0,
                    }),
                },
            ],
        };

        let mut live_set = Vec::new();
        fun.live_set_into(&mut live_set);

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
