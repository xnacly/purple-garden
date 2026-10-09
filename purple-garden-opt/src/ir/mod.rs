//! IR-level optimisation passes.
//!
//! Each pass lives in its own submodule. Orchestration (which passes
//! run, in what order) lives in [crate::ir] in `src/opt/mod.rs`.

mod addrof_fold;
mod branch_cmp;
mod const_fold;
mod const_fold_syscalls;
mod dce;
mod imm_fold;
mod indirect_jump;
mod load_store_fold;
mod ret_inline;
mod switch_fold;
mod switch_lookup;
mod tailcall;

use std::alloc::{Allocator, Global};

use purple_garden_ir::{self as ir, Id};

/// Location of a recorded `LoadConst`.
#[derive(Clone, Copy)]
pub struct ConstDef {
    block: u32,
    instr: u32,
}

/// Shared analysis state for IR-level passes that need SSA use counts
/// plus a map of fold-eligible `LoadConst` defs. One instance is
/// reused across every function in a compile (reset by [`Scratch::reset`]) on
/// `uses` and `consts`) so the allocation amortises.
///
/// Both vecs are indexed by `Id.0`. Invariant: `uses.len() ==
/// consts.len()`; [`Scratch::ensure`] is the only place they grow,
/// and they grow together so callers can index either side without
/// bounds-checking the other.
pub struct Scratch<'scratch, S: Allocator = Global> {
    uses: Vec<u32, &'scratch S>,
    consts: Vec<Option<ConstDef>, &'scratch S>,
}

impl Default for Scratch<'_> {
    fn default() -> Self {
        Self::new_in(&Global)
    }
}

impl<'scratch, S: Allocator> Scratch<'scratch, S> {
    pub fn new_in(scratch: &'scratch S) -> Self {
        Self {
            uses: Vec::new_in(scratch),
            consts: Vec::new_in(scratch),
        }
    }

    /// The allocator behind the analysis, for a pass's own temporaries.
    pub fn alloc(&self) -> &'scratch S {
        self.uses.allocator()
    }

    /// Clear all recorded analysis while retaining vector capacity.
    pub fn reset(&mut self) {
        self.uses.clear();
        self.consts.clear();
    }

    /// Returns the recorded `ConstDef` for `id`, no use-count gate.
    /// `const_fold` uses this; `imm_fold` uses `single_use_const` instead
    /// because it noops the `LoadConst` and needs single-use safety.
    pub fn const_def(&self, id: Id) -> Option<ConstDef> {
        self.consts.get(id.0 as usize).copied().flatten()
    }

    /// Grow both vecs to cover `id`, preserving the parallel-length
    /// invariant.
    pub fn ensure(&mut self, id: Id) {
        let len = id.0 as usize + 1;
        if self.uses.len() < len {
            self.uses.resize(len, 0);
            self.consts.resize(len, None);
        }
    }

    /// Record where a `LoadConst` defines `id`.
    pub fn record_const(&mut self, id: Id, block: u32, instr: u32) {
        self.ensure(id);
        self.consts[id.0 as usize] = Some(ConstDef { block, instr });
    }

    /// Record one use of `id`. After the analyze pass `uses[id.0]` is
    /// the total use count of that SSA value across the whole function.
    pub fn bump(&mut self, id: Id) {
        self.ensure(id);
        self.uses[id.0 as usize] += 1;
    }

    /// Returns the total use count for `id`, or `0` if the slot was never
    /// touched in this analysis pass.
    pub fn use_count(&self, id: Id) -> u32 {
        self.uses.get(id.0 as usize).copied().unwrap_or(0)
    }

    /// Returns the `ConstDef` for `id` iff it was defined by a
    /// `LoadConst` AND has exactly one use. The single-use check is the
    /// fold-safety gate: with >1 uses the `LoadConst` is still needed
    /// elsewhere and noop'ing it would corrupt those uses.
    pub fn single_use_const(&self, id: Id) -> Option<ConstDef> {
        let idx = id.0 as usize;
        if self.uses.get(idx).copied() != Some(1) {
            return None;
        }
        self.consts[idx]
    }
}

/// Recompute whole-function SSA use counts in `scratch`.
///
/// This also calls [`Scratch::ensure`] for definitions with zero uses, so
/// callers can distinguish "defined but dead" from "id never seen" when needed.
pub(super) fn record_uses<S: Allocator>(fun: &ir::Func<'_>, scratch: &mut Scratch<'_, S>) {
    scratch.reset();

    for block in &fun.blocks {
        if block.tombstone {
            continue;
        }

        for instr in &block.instructions {
            if let Some(id) = ir::Func::def_of(instr) {
                scratch.ensure(id);
            }
            ir::Func::for_each_use_of_instr(instr, |id| scratch.bump(id));
        }

        if let Some(term) = &block.term {
            fun.for_each_use_of_term(term, |id| scratch.bump(id));
        }
    }
}

/// Collects `items` into `scratch`, `None` as soon as one item is `None`.
pub(super) fn try_collect_in<T, S: Allocator>(
    items: impl IntoIterator<Item = Option<T>>,
    scratch: S,
) -> Option<Vec<T, S>> {
    let mut out = Vec::new_in(scratch);
    for item in items {
        out.push(item?);
    }
    Some(out)
}

/// Number of live edges into each block, indexed by block id.
pub(super) fn predecessor_counts<S: Allocator>(fun: &ir::Func, scratch: S) -> Vec<u32, S> {
    let mut counts = Vec::with_capacity_in(fun.blocks.len(), scratch);
    counts.resize(fun.blocks.len(), 0);

    for block in &fun.blocks {
        if block.tombstone {
            continue;
        }

        match &block.term {
            Some(ir::Terminator::Jump { id, .. }) => counts[id.0 as usize] += 1,
            Some(ir::Terminator::Branch { yes, no, .. })
            | Some(ir::Terminator::BranchCmpImm { yes, no, .. })
            | Some(ir::Terminator::BranchCmp { yes, no, .. }) => {
                counts[yes.0.0 as usize] += 1;
                counts[no.0.0 as usize] += 1;
            }
            Some(ir::Terminator::Switch { cases, default, .. }) => {
                for case in fun.cases(*cases) {
                    counts[case.target.0.0 as usize] += 1;
                }
                counts[default.0.0 as usize] += 1;
            }
            _ => {}
        }
    }

    counts
}

// reexports
pub use addrof_fold::addrof_fold;
pub use branch_cmp::branch_cmp;
pub use const_fold::const_fold;
pub use const_fold_syscalls::const_fold_syscalls;
pub use dce::dce;
pub use imm_fold::imm_fold;
pub use indirect_jump::indirect_jump;
pub use load_store_fold::load_store_fold;
pub use ret_inline::ret_inline;
pub use switch_fold::switch_fold;
pub use switch_lookup::switch_lookup;
pub use tailcall::tailcall;
