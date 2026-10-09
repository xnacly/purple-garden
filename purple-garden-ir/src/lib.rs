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
#![feature(allocator_api)]

pub mod constant;
mod display;
pub mod ptype;

use std::alloc::{Allocator, Layout};

pub use crate::constant::Const;
use crate::ptype::Type;
use purple_garden_shared::BuiltinFn;

pub type ConstEvalFn = for<'args, 'c> fn(&'args [Const<'c>]) -> Option<Const<'c>>;

#[derive(Debug, Clone)]
pub struct Fn<'f> {
    pub name: &'f str,
    pub doc: &'f str,
    pub ptr: BuiltinFn,
    pub pure: bool,
    pub eval: Option<ConstEvalFn>,
    pub arg_names: &'f [&'f str],
    pub args: &'f [Type<'f>],
    pub ret: Type<'f>,
    /// Overload group this fn specialises, e.g. `println_int` specialises
    /// `println`. `Some` => callable only via the group name
    pub specialises: Option<&'f str>,
    pub with_slots: bool,
}

impl<'f> Fn<'f> {
    /// Script-facing name: the overload group when this fn `specialises` one,
    /// otherwise its own name. The single rule for grouping specialisations,
    /// shared by typecheck, lowering, and doc rendering.
    #[must_use]
    pub fn group_name(&self) -> &'f str {
        self.specialises.unwrap_or(self.name)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(pub u32);

/// Where a backend placed an SSA value, indexed by [`Id`]. Produced by the
/// bytecode backend's register allocator and consumed by every code generator
/// (bytecode emit, JIT); hence it lives here in the shared IR vocabulary
/// rather than in any single backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    /// Slot has no interval (id is unused, e.g. a tombstoned block's param).
    /// Reading these from a backend's location map is a compiler bug.
    Unassigned,
    Reg(u8),
    Stack,
}

/// Index into [`Func::params_pool`]. Stands in wherever a block-param
/// list used to live by value (`Vec<Id>`): in `Block.params` and in
/// every `Terminator` arm that carries successor params.
///
/// `Copy + 4 bytes`, so sharing a list across many sites (e.g. the four
/// sinks per match case in `Lower::lower_node`'s `Node::Match` arm) is
/// a `u32` copy. The actual `[Id]` data lives once in the function-level
/// pool; see [`Func::params_pool`] for the full discipline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParamsId(pub u32);

/// Sentinel for "this block has no params yet". Always slot 0 of every
/// function's pool, seeded by [`Func::new`]. Lets `Lower::new_block`
/// hand out a valid id at construction time before real params are known.
pub const EMPTY_PARAMS: ParamsId = ParamsId(0);

/// Handle to a [`Terminator::Switch`]'s cases in [`Func::cases_pool`], same
/// discipline as [`ParamsId`]. Keeps the keys' lifetime out of `Terminator`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CasesId(pub u32);

/// Handle to an [`Instr::Lookup`]'s entries in [`Func::entries_pool`], same
/// discipline as [`ParamsId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntriesId(pub u32);

#[derive(Debug, Clone)]
pub struct Case<'c> {
    pub key: Const<'c>,
    pub target: (Id, ParamsId),
}

#[derive(Debug, Clone)]
pub struct TypeId<'t> {
    pub id: Id,
    pub ty: Type<'t>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    IAdd,
    ISub,
    IMul,
    IDiv,
    IMod,
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
    SEq,
}

#[derive(Debug, Clone)]
pub enum Instr<'i> {
    Bin {
        op: BinOp,
        dst: TypeId<'i>,
        lhs: Id,
        rhs: Id,
        /// Byte offset into the source of the originating AST node. Threaded
        /// through to `bc::Cc::pc_to_span` so runtime traps render with the
        /// right `file:line:col`. See `Vm::pc_to_span`.
        span: u32,
    },
    BinImm {
        op: BinOp,
        dst: TypeId<'i>,
        lhs: Id,
        imm: i32,
        span: u32,
    },
    LoadConst {
        dst: TypeId<'i>,
        value: Const<'i>,
        span: u32,
    },
    Call {
        dst: TypeId<'i>,
        func: Id,
        args: ParamsId,
        span: u32,
    },
    Sys {
        dst: TypeId<'i>,
        path: &'i str,
        fun: &'i Fn<'i>,
        args: ParamsId,
        span: u32,
    },
    Cast {
        dst: TypeId<'i>,
        from: TypeId<'i>,
        span: u32,
    },
    Alloc {
        dst: TypeId<'i>,
        layout: Layout,
        span: u32,
    },
    Store {
        src: Id,
        base: Id,
        offset: u32,
        span: u32,
    },
    Load {
        dst: TypeId<'i>,
        base: Id,
        offset: u32,
        span: u32,
    },
    AddrOf {
        dst: TypeId<'i>,
        base: Id,
        offset: u32,
        span: u32,
    },
    /// `dst` is the value of the entry whose key equals `subject`, else
    /// `default`: a switch whose arms only return constants.
    Lookup {
        dst: TypeId<'i>,
        subject: Id,
        entries: EntriesId,
        default: Const<'i>,
        span: u32,
    },
    Noop,
}

impl Instr<'_> {
    /// Source byte offset for this instruction, or 0 for synthetic Noops
    /// (which never trap, so the missing span doesn't matter).
    #[must_use]
    pub fn span(&self) -> u32 {
        match self {
            Instr::Store { span, .. }
            | Instr::Load { span, .. }
            | Instr::AddrOf { span, .. }
            | Instr::Bin { span, .. }
            | Instr::Alloc { span, .. }
            | Instr::BinImm { span, .. }
            | Instr::LoadConst { span, .. }
            | Instr::Call { span, .. }
            | Instr::Sys { span, .. }
            | Instr::Cast { span, .. }
            | Instr::Lookup { span, .. } => *span,
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
        /// Index into the enclosing [`Func::params_pool`] holding the
        /// successor-block param values passed by this jump. `Copy`, so
        /// the match lowering can hand the same `ParamsId` to every
        /// case's Branch + body Block without cloning the underlying
        /// `[Id]`. Dereference with `func.params(self.params)`.
        params: ParamsId,
        span: u32,
    },
    Branch {
        cond: Id,
        yes: (Id, ParamsId),
        no: (Id, ParamsId),
        span: u32,
    },
    BranchCmpImm {
        op: BinOp,
        lhs: Id,
        imm: i32,
        yes: (Id, ParamsId),
        no: (Id, ParamsId),
        span: u32,
    },
    BranchCmp {
        op: BinOp,
        lhs: Id,
        rhs: Id,
        yes: (Id, ParamsId),
        no: (Id, ParamsId),
        span: u32,
    },
    Tail {
        func: Id,
        args: ParamsId,
        span: u32,
    },
    /// Jump to the first case whose key equals `subject`, `default` if none
    /// does. Produced by `opt::ir::switch_fold` from a chain of equality
    /// branches; keys are Int or Str constants.
    Switch {
        subject: Id,
        cases: CasesId,
        default: (Id, ParamsId),
        span: u32,
    },
}

impl Terminator {
    #[must_use]
    pub fn span(&self) -> u32 {
        match self {
            Terminator::Return { span, .. }
            | Terminator::Jump { span, .. }
            | Terminator::Branch { span, .. }
            | Terminator::BranchCmpImm { span, .. }
            | Terminator::BranchCmp { span, .. }
            | Terminator::Tail { span, .. }
            | Terminator::Switch { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Block<'b> {
    /// block is dead as a result of optimisation passes
    ///
    /// ```text
    ///        ,-=-.       ______     _
    ///       /  +  \     />----->  _|1|_
    ///       | ~~~ |    // -/- /  |_ H _|
    ///       |R.I.P|   //  /  /     |S|
    ///  \vV,,|_____|V,//_____/VvV,v,|_|/,,vhjwv/,
    /// ```
    ///
    /// It will not be revived :)
    pub tombstone: bool,
    pub id: Id,
    pub instructions: Vec<Instr<'b>>,
    /// Index into the enclosing [`Func::params_pool`] holding this
    /// block's SSA params (the values defined on entry, used as
    /// phi-equivalents). 4-byte `Copy`. Dereference with
    /// `func.params(block.params)`.
    pub params: ParamsId,
    /// each block has a term, but a block starts without one in the lowering process, thus this
    /// field has to be optional
    pub term: Option<Terminator>,
}

#[derive(Debug, Clone, Default)]
pub struct Func<'f> {
    pub name: &'f str,
    pub id: Id,
    pub span: u32,
    pub params: Vec<Id>,
    pub ret: Option<Type<'f>>,
    pub blocks: Vec<Block<'f>>,
    /// Owning storage for every block-param / Branch-arg / Jump-arg
    /// `[Id]` list this function references. Together with [`ParamsId`]
    /// this is the static (no-Rc, no-Arc) equivalent of shared-by-handle
    /// param lists:
    ///
    /// - `Block.params: ParamsId` and the `params` / `yes.1` / `no.1`
    ///   fields of [`Terminator`] are all 4-byte indices into this Vec.
    /// - Multiple sites can hold the same `ParamsId`; they all read the
    ///   same `[Id]` via `func.params(id)`. No deduplication; two
    ///   `intern_params` calls with the same `Vec<Id>` get two slots.
    ///   Sharing happens only because the same `ParamsId` is handed to
    ///   multiple sinks at the intern site.
    /// - Slot 0 is always the empty slice (seeded by [`Func::new`]),
    ///   referenced by [`EMPTY_PARAMS`]. New blocks start with that and
    ///   get rewritten when their actual params are known.
    ///
    /// This is the same discipline as `bc::Cc::block_map` and
    /// `Ralloc.map`: data hangs off a long-lived owner, handles are
    /// `Copy` `u32`
    pub params_pool: Pool<Id>,
    /// Owning storage for [`Terminator::Switch`] cases, see [`CasesId`].
    pub cases_pool: Pool<Case<'f>>,
    /// Owning storage for [`Instr::Lookup`] entries, see [`EntriesId`].
    pub entries_pool: Pool<(Const<'f>, Const<'f>)>,
}

/// The lists of one kind of a function, back to back in one allocation and
/// addressed by the index [`Pool::intern`] returns. Interned lists never
/// change, a rewrite interns a new one and the old stays until the function is
/// dropped.
#[derive(Debug, Clone)]
pub struct Pool<T> {
    items: Vec<T>,
    /// `(start, len)` into `items` per interned list.
    spans: Vec<(u32, u32)>,
}

impl<T> Default for Pool<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            spans: Vec::new(),
        }
    }
}

impl<T> Pool<T> {
    pub fn intern(&mut self, list: impl IntoIterator<Item = T>) -> u32 {
        let start = self.items.len();
        self.items.extend(list);
        self.spans
            .push((start as u32, (self.items.len() - start) as u32));
        self.spans.len() as u32 - 1
    }

    #[must_use]
    pub fn get(&self, id: u32) -> &[T] {
        let (start, len) = self.spans[id as usize];
        &self.items[start as usize..][..len as usize]
    }
}

impl<'f> Func<'f> {
    /// Build a new `Func` with `params_pool[0]` already seeded with the
    /// empty slice (the [`EMPTY_PARAMS`] sentinel). Always go through
    /// this; a `Func` whose pool is empty would make `EMPTY_PARAMS` an
    /// out-of-bounds lookup.
    #[must_use]
    pub fn new(name: &'f str, id: Id, params: Vec<Id>, ret: Option<Type<'f>>) -> Self {
        Self {
            name,
            id,
            span: 0,
            params,
            ret,
            blocks: Vec::new(),
            params_pool: {
                let mut pool = Pool::default();
                pool.intern([]);
                pool
            },
            cases_pool: Pool::default(),
            entries_pool: Pool::default(),
        }
    }

    #[must_use]
    pub fn with_span(mut self, span: u32) -> Self {
        self.span = span;
        self
    }

    pub fn intern_params(&mut self, params: impl IntoIterator<Item = Id>) -> ParamsId {
        ParamsId(self.params_pool.intern(params))
    }

    #[must_use]
    pub fn params(&self, id: ParamsId) -> &[Id] {
        self.params_pool.get(id.0)
    }

    pub fn intern_cases(&mut self, cases: impl IntoIterator<Item = Case<'f>>) -> CasesId {
        CasesId(self.cases_pool.intern(cases))
    }

    #[must_use]
    pub fn cases(&self, id: CasesId) -> &[Case<'f>] {
        self.cases_pool.get(id.0)
    }

    pub fn intern_entries(
        &mut self,
        entries: impl IntoIterator<Item = (Const<'f>, Const<'f>)>,
    ) -> EntriesId {
        EntriesId(self.entries_pool.intern(entries))
    }

    #[must_use]
    pub fn entries(&self, id: EntriesId) -> &[(Const<'f>, Const<'f>)] {
        self.entries_pool.get(id.0)
    }
}

impl Func<'_> {
    #[must_use]
    pub fn def_of(instr: &Instr<'_>) -> Option<Id> {
        match instr {
            Instr::Alloc { dst, .. }
            | Instr::Load { dst, .. }
            | Instr::AddrOf { dst, .. }
            | Instr::Bin { dst, .. }
            | Instr::BinImm { dst, .. }
            | Instr::LoadConst { dst, .. }
            | Instr::Call { dst, .. }
            | Instr::Sys { dst, .. }
            | Instr::Cast { dst, .. }
            | Instr::Lookup { dst, .. } => Some(dst.id),
            Instr::Store { .. } | Instr::Noop => None,
        }
    }

    pub fn for_each_use_of_instr(&self, instr: &Instr<'_>, mut f: impl FnMut(Id)) {
        match instr {
            Instr::Bin { lhs, rhs, .. } => {
                f(*lhs);
                f(*rhs);
            }
            Instr::BinImm { lhs, .. } => f(*lhs),
            Instr::Call { args, .. } | Instr::Sys { args, .. } => {
                for &a in self.params(*args) {
                    f(a);
                }
            }
            Instr::Cast { from, .. } => f(from.id),
            Instr::Lookup { subject, .. } => f(*subject),
            Instr::Store { src, base, .. } => {
                f(*src);
                f(*base);
            }
            Instr::Load { base, .. } | Instr::AddrOf { base, .. } => f(*base),
            Instr::LoadConst { .. } | Instr::Noop | Instr::Alloc { .. } => {}
        }
    }

    pub fn for_each_use_of_term(&self, term: &Terminator, mut f: impl FnMut(Id)) {
        match term {
            Terminator::Return {
                value: Some(id), ..
            } => f(*id),
            Terminator::Return { value: None, .. } => {}
            Terminator::Jump { params, .. } => {
                for &p in self.params(*params) {
                    f(p);
                }
            }
            Terminator::Tail { args, .. } => {
                for &a in self.params(*args) {
                    f(a);
                }
            }
            Terminator::Switch {
                subject,
                cases,
                default: (_, default_params),
                ..
            } => {
                f(*subject);
                for case in self.cases(*cases) {
                    for &p in self.params(case.target.1) {
                        f(p);
                    }
                }
                for &p in self.params(*default_params) {
                    f(p);
                }
            }
            Terminator::Branch {
                cond,
                yes: (_, yes_params),
                no: (_, no_params),
                ..
            } => {
                f(*cond);
                for &p in self.params(*yes_params) {
                    f(p);
                }
                for &p in self.params(*no_params) {
                    f(p);
                }
            }
            Terminator::BranchCmpImm {
                lhs,
                yes: (_, yes_params),
                no: (_, no_params),
                ..
            }
            | Terminator::BranchCmp {
                lhs,
                yes: (_, yes_params),
                no: (_, no_params),
                ..
            } => {
                f(*lhs);
                if let Terminator::BranchCmp { rhs, .. } = term {
                    f(*rhs);
                }
                for &p in self.params(*yes_params) {
                    f(p);
                }
                for &p in self.params(*no_params) {
                    f(p);
                }
            }
        }
    }

    /// Per-SSA live interval, indexed by id. `(u32::MAX, 0)` marks a slot with no def; only happens
    /// for params of tombstoned blocks since SSA ids are otherwise dense.
    ///
    /// Writes into `out`, clearing first. Lets the caller reuse a buffer across function compiles
    /// so we don't allocate fresh per `cc()`.
    pub fn live_set_into<A: Allocator>(&self, out: &mut Vec<(u32, u32), A>) {
        const UNSET: (u32, u32) = (u32::MAX, 0);

        fn ensure<A: Allocator>(v: &mut Vec<(u32, u32), A>, id: u32) {
            let idx = id as usize;
            if idx >= v.len() {
                v.resize(idx + 1, UNSET);
            }
        }

        fn define<A: Allocator>(intervals: &mut Vec<(u32, u32), A>, id: Id, pos: u32) {
            ensure(intervals, id.0);
            let e = &mut intervals[id.0 as usize];
            if e.0 == u32::MAX {
                *e = (pos, pos);
            } else {
                e.0 = e.0.min(pos);
                e.1 = e.1.max(pos);
            }
        }

        fn use_value<A: Allocator>(intervals: &mut Vec<(u32, u32), A>, id: Id, pos: u32) {
            ensure(intervals, id.0);
            let e = &mut intervals[id.0 as usize];
            if e.0 == u32::MAX {
                *e = (pos, pos);
            } else {
                e.1 = e.1.max(pos);
            }
        }

        out.clear();

        // Each event takes two slots: early (pos) and late (pos+1). Instr
        // sources land on early, dst on late, so a source dying at pos P
        // and a dst born at pos P can share a register. Branch yes-shuffle
        // is early, cond and no-shuffle are late, so yes_movs (which run
        // before JmpT) can't clobber cond. bc::Cc::cc steps pos by 2 in
        // lockstep so its call-site spill checks see the same units.
        let intervals = &mut *out;
        let mut pos = 0;

        for block in &self.blocks {
            if block.tombstone {
                continue;
            }

            // A header redeclaring an already live param writes nothing, the
            // edge move into it happened in the predecessor, so it must not
            // extend the interval past the param's last real use.
            for param in self.params(block.params) {
                ensure(intervals, param.0);
                let e = &mut intervals[param.0 as usize];
                if e.0 == u32::MAX {
                    *e = (pos, pos);
                } else {
                    e.0 = e.0.min(pos);
                }
            }
            pos += 2;

            for instr in &block.instructions {
                self.for_each_use_of_instr(instr, |use_id| {
                    use_value(intervals, use_id, pos);
                });

                if let Some(def_id) = Self::def_of(instr) {
                    define(intervals, def_id, pos + 1);
                }
                pos += 2;
            }

            if let Some(term) = &block.term {
                match term {
                    Terminator::Branch {
                        cond,
                        yes: (yes_id, yes_params),
                        no: (no_id, no_params),
                        ..
                    } => {
                        // yes-shuffle runs before JmpT.
                        for &p in self.params(*yes_params) {
                            use_value(intervals, p, pos);
                        }
                        for &p in self.params(self.blocks[yes_id.0 as usize].params) {
                            define(intervals, p, pos);
                        }
                        // JmpT reads cond after yes_movs; the phase gap
                        // keeps cond's reg out of yes_dst's reach.
                        use_value(intervals, *cond, pos + 1);
                        // no-shuffle runs after JmpT.
                        for &p in self.params(*no_params) {
                            use_value(intervals, p, pos + 1);
                        }
                        for &p in self.params(self.blocks[no_id.0 as usize].params) {
                            define(intervals, p, pos + 1);
                        }
                    }
                    Terminator::BranchCmpImm {
                        lhs,
                        yes: (yes_id, yes_params),
                        no: (no_id, no_params),
                        ..
                    }
                    | Terminator::BranchCmp {
                        lhs,
                        yes: (yes_id, yes_params),
                        no: (no_id, no_params),
                        ..
                    } => {
                        // Same edge-move phasing as Branch: yes shuffle first,
                        // comparison operands after yes moves, no shuffle last.
                        for &p in self.params(*yes_params) {
                            use_value(intervals, p, pos);
                        }
                        for &p in self.params(self.blocks[yes_id.0 as usize].params) {
                            define(intervals, p, pos);
                        }
                        use_value(intervals, *lhs, pos + 1);
                        if let Terminator::BranchCmp { rhs, .. } = term {
                            use_value(intervals, *rhs, pos + 1);
                        }
                        for &p in self.params(*no_params) {
                            use_value(intervals, p, pos + 1);
                        }
                        for &p in self.params(self.blocks[no_id.0 as usize].params) {
                            define(intervals, p, pos + 1);
                        }
                    }
                    // Jump/Tail shuffle dsts are NOT recorded: doing so
                    // would extend a join-block param back through every
                    // predecessor, making the call-site spill check
                    // spill (and pop-overwrite) the very value the call
                    // is computing. The IR mostly threads matching SSA
                    // ids so jump shuffles elide; the residual hazard is
                    // a known TODO (parallel-move resolver).
                    _ => {
                        self.for_each_use_of_term(term, |use_id| {
                            use_value(intervals, use_id, pos);
                        });
                    }
                }
            }
            pos += 2;
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
    pub fn arg_hints_into<A: Allocator>(&self, hints: &mut Vec<Option<u8>, A>) {
        fn ensure<A: Allocator>(v: &mut Vec<Option<u8>, A>, id: u32) {
            let idx = id as usize;
            if idx >= v.len() {
                v.resize(idx + 1, None);
            }
        }

        // First-hint-wins: skip if already set.
        fn put<A: Allocator>(v: &mut Vec<Option<u8>, A>, id: Id, reg: u8) {
            ensure(v, id.0);
            let e = &mut v[id.0 as usize];
            if e.is_none() {
                *e = Some(reg);
            }
        }

        // Overwrite unconditionally; used for entry-block params so they
        // beat any subsequent inner-call hint and stay pinned to the
        // calling convention's r0..r{N-1}.
        fn put_force<A: Allocator>(v: &mut Vec<Option<u8>, A>, id: Id, reg: u8) {
            ensure(v, id.0);
            v[id.0 as usize] = Some(reg);
        }

        hints.clear();

        // Entry block params arrive in r0..r{N-1} per the calling convention
        // (ARM-like: r0 is both the first arg and the return-value slot).
        // Pin them first so they take priority over inner-call hints; otherwise
        // an inner call that uses the function's first param as its arg-2 would
        // hint it to r1, the regalloc would place it in r1, and the caller still
        // writes the arg to r0 → the function reads garbage.
        if let Some(entry) = self.blocks.first()
            && !entry.tombstone
        {
            for (i, param) in self.params(entry.params).iter().enumerate() {
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
                        for (i, arg) in self.params(*args).iter().enumerate() {
                            put(hints, *arg, i as u8);
                        }
                        put(hints, dst.id, 0u8);
                    }
                    _ => {}
                }
            }
            if let Some(Terminator::Tail { args, .. }) = &block.term {
                for (i, arg) in self.params(*args).iter().enumerate() {
                    put(hints, *arg, i as u8);
                }
            }
        }

        // Back-propagate r0 from Return through Jump/Branch params, in
        // reverse so each pass picks up the prior block's hint. Lands the
        // return value directly in r0 when regalloc can honor it.
        for block in self.blocks.iter().rev() {
            if block.tombstone {
                continue;
            }
            match &block.term {
                Some(Terminator::Return {
                    value: Some(id), ..
                }) => {
                    put(hints, *id, 0u8);
                }
                Some(Terminator::Jump {
                    id: target_id,
                    params,
                    ..
                }) => {
                    let target = &self.blocks[target_id.0 as usize];
                    if target.tombstone {
                        continue;
                    }
                    let src_params = self.params(*params);
                    let dst_params = self.params(target.params);
                    for (i, &src) in src_params.iter().enumerate() {
                        if let Some(&dst) = dst_params.get(i)
                            && let Some(Some(reg)) = hints.get(dst.0 as usize).copied()
                        {
                            put(hints, src, reg);
                        }
                    }
                }
                Some(Terminator::Branch { yes, no, .. })
                | Some(Terminator::BranchCmpImm { yes, no, .. })
                | Some(Terminator::BranchCmp { yes, no, .. }) => {
                    for (target_id, params) in [yes, no] {
                        let target = &self.blocks[target_id.0 as usize];
                        if target.tombstone {
                            continue;
                        }
                        let src_params = self.params(*params);
                        let dst_params = self.params(target.params);
                        for (i, &src) in src_params.iter().enumerate() {
                            if let Some(&dst) = dst_params.get(i)
                                && let Some(Some(reg)) = hints.get(dst.0 as usize).copied()
                            {
                                put(hints, src, reg);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ptype::Type, *};

    fn type_id(id: u32) -> TypeId<'static> {
        TypeId {
            id: Id(id),
            ty: Type::Int,
        }
    }

    #[test]
    fn live_set_tracks_block_params_instructions_and_terminators() {
        let mut fun = Func::new("live", Id(0), vec![Id(0)], Some(Type::Int));
        let b0_params = fun.intern_params(vec![Id(0)]);
        let b1_params = fun.intern_params(vec![Id(3)]);
        let b2_params = fun.intern_params(vec![Id(4)]);
        let branch_yes = fun.intern_params(vec![Id(0)]);
        let branch_no = fun.intern_params(vec![Id(1)]);
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params: b0_params,
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
                    yes: (Id(1), branch_yes),
                    no: (Id(2), branch_no),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params: b1_params,
                instructions: vec![],
                term: Some(Terminator::Return {
                    value: Some(Id(3)),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(2),
                params: b2_params,
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
        ];

        let mut live_set = Vec::new();
        fun.live_set_into(&mut live_set);

        // Each logical event consumes two pos slots (early + late). Within
        // an instruction, src use lands on early, dst def on late. Within
        // a Branch term, yes-shuffle is early, cond + no-shuffle are late.
        assert_eq!(live_set[0], (0, 6)); // %v0: b0 param, used by IAdd and yes-shuffle
        assert_eq!(live_set[1], (3, 7)); // %v1: defined by LoadConst (late), used by IAdd, no-shuffle
        assert_eq!(live_set[2], (5, 7)); // %v2: defined by IAdd (late), used by Branch cond (late)
        assert_eq!(live_set[3], (6, 10)); // %v3: yes-shuffle dst + b1 param, used by Return
        assert_eq!(live_set[4], (7, 14)); // %v4: no-shuffle dst + b2 param, used by Cast
        assert_eq!(live_set[5], (15, 16)); // %v5: Cast dst (late), used by Return
    }
}
