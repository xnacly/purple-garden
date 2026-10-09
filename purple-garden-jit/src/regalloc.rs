//! Forward register allocator for the JIT:
//!
//! - Registers are handed out while the backend emits code
//! - A value takes a free register when first touched and keeps it for its whole interval
//! - A register returns to the free list once the interval holding it ends or before the current allocation point
//!
//! For instructions clobbering fixed registers (such as helper calls, or x86 `idiv`)  the live
//! values it would clobber must be copied into free registers around the instruction and again
//! copied back.
//!
//! xralloc2 is target-independent by allowing a pool of registers to be defined

use std::alloc::{Allocator, Global};

use purple_garden_ir::Id;

#[derive(Debug, Clone)]
pub struct Xralloc2<S: Allocator = Global> {
    map: Vec<Option<u8>, S>,
    /// `(last_use, reg)` of every value currently holding a register.
    active: Vec<(u32, u8), S>,
    free: Vec<u8, S>,
    /// Bitmask of every register handed out since [`Xralloc2::reset`].
    used: u32,
}

impl Default for Xralloc2 {
    fn default() -> Self {
        Self::new_in(Global)
    }
}

impl<S: Allocator + Clone> Xralloc2<S> {
    pub fn new_in(scratch: S) -> Self {
        Self {
            map: Vec::new_in(scratch.clone()),
            active: Vec::new_in(scratch.clone()),
            free: Vec::new_in(scratch),
            used: 0,
        }
    }

    /// `pool` is popped from the back, so the preferred registers go last.
    pub fn reset(&mut self, ids: usize, pool: &[u8]) {
        self.map.clear();
        self.map.resize(ids, None);
        self.active.clear();
        self.free.clear();
        self.free.extend_from_slice(pool);
        self.used = 0;
    }

    pub fn reg(&self, id: Id) -> Option<u8> {
        self.map.get(id.0 as usize).copied().flatten()
    }

    /// Assign `id` a register, reusing those of values whose last use is at or
    /// before `at`. Idempotent for already assigned ids.
    pub fn alloc(&mut self, id: Id, at: u32, last_use: u32) -> Option<u8> {
        if let Some(reg) = self.reg(id) {
            return Some(reg);
        }
        let Self { active, free, .. } = self;
        active.retain(|&(end, reg)| {
            if end <= at {
                free.push(reg);
            }
            end > at
        });
        let reg = self.free.pop()?;
        self.active.push((last_use, reg));
        *self.map.get_mut(id.0 as usize)? = Some(reg);
        self.used |= 1 << reg;
        Some(reg)
    }

    /// Registers holding values still needed after `pos`.
    pub fn live_after(&self, pos: u32) -> impl Iterator<Item = u8> + '_ {
        self.active
            .iter()
            .filter(move |&&(end, _)| end > pos)
            .map(|&(_, reg)| reg)
    }

    /// Borrow a free register matching `pred` until [`Xralloc2::give_back`].
    pub fn take_free(&mut self, pred: impl Fn(u8) -> bool) -> Option<u8> {
        let i = self.free.iter().rposition(|&reg| pred(reg))?;
        let reg = self.free.swap_remove(i);
        self.used |= 1 << reg;
        Some(reg)
    }

    pub fn give_back(&mut self, reg: u8) {
        self.free.push(reg);
    }

    pub fn used(&self, reg: u8) -> bool {
        self.used & (1 << reg) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::Xralloc2;
    use purple_garden_ir::Id;

    /// A value dying at `at` frees its register for the one defined there,
    /// which is what lets `d = l + r` reuse `l`'s register.
    #[test]
    fn dying_value_hands_its_register_to_the_next_def() {
        let mut ra = Xralloc2::default();
        ra.reset(2, &[1, 0]);
        assert_eq!(ra.alloc(Id(0), 0, 4), Some(0));
        assert_eq!(ra.alloc(Id(1), 4, 6), Some(0));
    }
}
