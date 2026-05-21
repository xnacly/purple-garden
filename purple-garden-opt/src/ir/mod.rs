//! IR-level optimisation passes.
//!
//! Each pass lives in its own submodule. Orchestration (which passes
//! run, in what order) lives in [crate::ir] in `src/opt/mod.rs`.

mod const_fold;
mod imm_fold;
mod indirect_jump;
mod ret_inline;
mod tailcall;

use purple_garden_ir::{Id, constant::Const};

/// A `LoadConst { dst, Int(value) }` recorded for possible folding.
/// `block`/`instr` are a backpointer to the original `LoadConst` so a
/// caller can overwrite it with `Noop` once it's been folded away.
/// `value` is stored at its native IR width (`i64`); consumers that
/// need a narrower form (e.g. `imm_fold` lowering to `i32` bytecode
/// immediates) narrow at fold time and skip the fold on overflow.
#[derive(Clone, Copy)]
pub struct ConstDef<'def> {
    value: Const<'def>,
    block: u32,
    instr: u32,
}

/// Shared analysis state for IR-level passes that need SSA use counts
/// plus a map of fold-eligible `LoadConst` defs. One instance is
/// reused across every function in a compile (reset by `clear()` on
/// `uses` and `consts`) so the allocation amortises.
///
/// Both vecs are indexed by `Id.0`. Invariant: `uses.len() ==
/// consts.len()`; [`Scratch::ensure`] is the only place they grow,
/// and they grow together so callers can index either side without
/// bounds-checking the other.
#[derive(Default)]
pub struct Scratch<'scratch> {
    uses: Vec<u32>,
    consts: Vec<Option<ConstDef<'scratch>>>,
}

impl Scratch<'_> {
    pub fn reset(&mut self) {
        self.uses.clear();
        self.consts.clear();
    }

    /// Returns the recorded `ConstDef` for `id`, no use-count gate.
    /// `const_fold` uses this; `imm_fold` uses `single_use_const` instead
    /// because it noops the `LoadConst` and needs single-use safety.
    pub fn const_def(&self, id: Id) -> Option<ConstDef<'_>> {
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

    /// Record one use of `id`. After the analyze pass `uses[id.0]` is
    /// the total use count of that SSA value across the whole function.
    pub fn bump(&mut self, id: Id) {
        self.ensure(id);
        self.uses[id.0 as usize] += 1;
    }

    /// Returns the `ConstDef` for `id` iff it was defined by a
    /// `LoadConst` AND has exactly one use. The single-use check is the
    /// fold-safety gate: with >1 uses the `LoadConst` is still needed
    /// elsewhere and noop'ing it would corrupt those uses.
    pub fn single_use_const(&self, id: Id) -> Option<ConstDef<'_>> {
        let idx = id.0 as usize;
        if self.uses.get(idx).copied() != Some(1) {
            return None;
        }
        self.consts[idx]
    }
}

// reexports
pub use const_fold::const_fold;
pub use imm_fold::imm_fold;
pub use indirect_jump::indirect_jump;
pub use ret_inline::ret_inline;
pub use tailcall::tailcall;
