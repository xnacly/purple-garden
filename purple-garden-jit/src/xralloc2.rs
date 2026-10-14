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
//! xralloc2 is target-independent by allowing a pool of registers to be defined per
//! [`Class`]: general purpose registers hold integers and pointers, floating point
//! registers hold doubles. The classes are separate register files, so a value in one
//! never conflicts with a value in the other.

use std::alloc::{Allocator, Global};

use purple_garden_ir::Id;
use purple_garden_runtime::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Gpr,
    Fpr,
}

impl From<&Type<'_>> for Class {
    fn from(value: &Type<'_>) -> Self {
        match value {
            Type::Double => Self::Fpr,
            _ => Self::Gpr,
        }
    }
}

/// A physical register, numbered within its class' register file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reg {
    Gpr(u8),
    Fpr(u8),
}

impl Reg {
    #[must_use]
    pub fn new(class: Class, num: u8) -> Self {
        match class {
            Class::Gpr => Self::Gpr(num),
            Class::Fpr => Self::Fpr(num),
        }
    }

    #[must_use]
    pub fn class(self) -> Class {
        match self {
            Self::Gpr(_) => Class::Gpr,
            Self::Fpr(_) => Class::Fpr,
        }
    }

    #[must_use]
    pub fn num(self) -> u8 {
        match self {
            Self::Gpr(num) | Self::Fpr(num) => num,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Xralloc2<S: Allocator = Global> {
    map: Vec<Option<Reg>, S>,
    /// `(last_use, reg)` of every value currently holding a register.
    active: Vec<(u32, Reg), S>,
    /// Free registers per [`Class`].
    free: [Vec<u8, S>; 2],
    /// Bitmask per [`Class`] of every register handed out since [`Xralloc2::reset`].
    used: [u32; 2],
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
            free: [Vec::new_in(scratch.clone()), Vec::new_in(scratch)],
            used: [0; 2],
        }
    }

    /// Pools are popped from the back, so the preferred registers go last.
    pub fn reset(&mut self, ids: usize, gprs: &[u8], fprs: &[u8]) {
        self.map.clear();
        self.map.resize(ids, None);
        self.active.clear();
        for (free, pool) in self.free.iter_mut().zip([gprs, fprs]) {
            free.clear();
            free.extend_from_slice(pool);
        }
        self.used = [0; 2];
    }

    pub fn reg(&self, id: Id) -> Option<Reg> {
        self.map.get(id.0 as usize).copied().flatten()
    }

    /// Assign `id` a register of `class`, reusing those of values whose last
    /// use is at or before `at`. Idempotent for already assigned ids.
    pub fn alloc(&mut self, id: Id, class: Class, at: u32, last_use: u32) -> Option<Reg> {
        if let Some(reg) = self.reg(id) {
            return Some(reg);
        }
        self.expire(at);
        let reg = Reg::new(class, self.free[class as usize].pop()?);
        self.active.push((last_use, reg));
        *self.map.get_mut(id.0 as usize)? = Some(reg);
        self.mark_used(reg);
        Some(reg)
    }

    /// Free the registers of values whose last use is at or before `at`.
    pub fn expire(&mut self, at: u32) {
        let Self { active, free, .. } = self;
        active.retain(|&(end, reg)| {
            if end <= at {
                free[reg.class() as usize].push(reg.num());
            }
            end > at
        });
    }

    /// Registers holding values still needed after `pos`.
    pub fn live_after(&self, pos: u32) -> impl Iterator<Item = Reg> + '_ {
        self.active
            .iter()
            .filter(move |&&(end, _)| end > pos)
            .map(|&(_, reg)| reg)
    }

    /// Borrow a free register of `class` matching `pred` until [`Xralloc2::give_back`].
    pub fn take_free(&mut self, class: Class, pred: impl Fn(u8) -> bool) -> Option<Reg> {
        let free = &mut self.free[class as usize];
        let i = free.iter().rposition(|&reg| pred(reg))?;
        let reg = Reg::new(class, free.swap_remove(i));
        self.mark_used(reg);
        Some(reg)
    }

    pub fn give_back(&mut self, reg: Reg) {
        self.free[reg.class() as usize].push(reg.num());
    }

    pub fn used(&self, reg: Reg) -> bool {
        self.used[reg.class() as usize] & (1 << reg.num()) != 0
    }

    fn mark_used(&mut self, reg: Reg) {
        self.used[reg.class() as usize] |= 1 << reg.num();
    }
}

#[cfg(test)]
mod tests {
    use super::{Class, Reg, Xralloc2};
    use purple_garden_ir::Id;

    /// A value dying at `at` frees its register for the one defined there,
    /// which is what lets `d = l + r` reuse `l`'s register.
    #[test]
    fn dying_value_hands_its_register_to_the_next_def() {
        let mut ra = Xralloc2::default();
        ra.reset(2, &[1, 0], &[]);
        assert_eq!(ra.alloc(Id(0), Class::Gpr, 0, 4), Some(Reg::Gpr(0)));
        assert_eq!(ra.alloc(Id(1), Class::Gpr, 4, 6), Some(Reg::Gpr(0)));
    }

    /// The classes are separate register files: a double and an integer
    /// never compete for a register, and a dying double only frees an fpr.
    #[test]
    fn classes_are_allocated_independently() {
        let mut ra = Xralloc2::default();
        ra.reset(4, &[0], &[0]);
        assert_eq!(ra.alloc(Id(0), Class::Gpr, 0, 10), Some(Reg::Gpr(0)));
        assert_eq!(ra.alloc(Id(1), Class::Fpr, 0, 2), Some(Reg::Fpr(0)));
        assert_eq!(ra.alloc(Id(2), Class::Gpr, 2, 4), None);
        assert_eq!(ra.alloc(Id(3), Class::Fpr, 2, 4), Some(Reg::Fpr(0)));
        assert!(ra.used(Reg::Gpr(0)));
        assert!(ra.used(Reg::Fpr(0)));
        assert!(!ra.used(Reg::Fpr(1)));
    }
}
