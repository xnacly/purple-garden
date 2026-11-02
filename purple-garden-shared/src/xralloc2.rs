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
//! registers hold doubles.
use std::alloc::{Allocator, Global};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Gpr,
    Fpr,
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
    /// Allocation order per [`Class`], the most preferred register last.
    pool: [Vec<u8, S>; 2],
    /// Bitmask per [`Class`] of the registers currently free.
    free: [u64; 2],
    /// Bitmask per [`Class`] of every register handed out since [`Xralloc2::reset`].
    used: [u64; 2],
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
            pool: [Vec::new_in(scratch.clone()), Vec::new_in(scratch)],
            free: [0; 2],
            used: [0; 2],
        }
    }

    pub fn reset(&mut self, ids: usize, gprs: &[u8], fprs: &[u8]) {
        self.map.clear();
        self.map.resize(ids, None);
        self.active.clear();
        for (class, regs) in [gprs, fprs].into_iter().enumerate() {
            self.pool[class].clear();
            self.pool[class].extend_from_slice(regs);
            self.free[class] = regs.iter().fold(0, |mask, &num| mask | 1u64 << num);
        }
        self.used = [0; 2];
    }

    pub fn reg(&self, id: impl Into<usize>) -> Option<Reg> {
        self.map.get(id.into()).copied().flatten()
    }

    /// Assign `id` a register of `class`, reusing those of values whose last
    /// use is at or before `at`
    pub fn alloc(
        &mut self,
        id: impl Into<usize>,
        class: Class,
        at: u32,
        last_use: u32,
    ) -> Option<Reg> {
        self.alloc_inner(id.into(), class, at, last_use, None)
    }

    /// [`Xralloc2::alloc`], but prioritising `hint` if it is free at `at`.
    pub fn alloc_with_hint(
        &mut self,
        id: impl Into<usize>,
        class: Class,
        at: u32,
        last_use: u32,
        hint: Reg,
    ) -> Option<Reg> {
        self.alloc_inner(id.into(), class, at, last_use, Some(hint))
    }

    fn alloc_inner(
        &mut self,
        id: usize,
        class: Class,
        at: u32,
        last_use: u32,
        hint: Option<Reg>,
    ) -> Option<Reg> {
        // A value keeps its register for its whole live interval, so if its marked as used and
        // requested again, its the same reg
        if let Some(reg) = self.reg(id) {
            return Some(reg);
        }

        // Values whose last use is at or before `at` are dead, their reg should be free again
        //
        // This is what enables reuse of the reg l is in for d in `d = l + r` (if l dead at at)
        self.expire(at);

        // The pool is ordered, so the preferred register (hint) must be at the back and be popped
        // as the allocated register
        let num = match hint {
            Some(hint) if hint.class() == class && self.try_take(hint).is_some() => hint.num(),
            _ => self.pop(class)?,
        };

        let reg = Reg::new(class, num);
        self.active.push((last_use, reg));
        self.map[id] = Some(reg);
        self.mark_used(reg);
        Some(reg)
    }

    /// Assign `id` the free register `reg`, where a parameter arrives. `None` if `reg` isn't free.
    pub fn try_alloc_at(&mut self, id: impl Into<usize>, reg: Reg, last_use: u32) -> Option<Reg> {
        self.try_take(reg)?;
        self.active.push((last_use, reg));
        self.map[id.into()] = Some(reg);
        Some(reg)
    }

    /// Pick a register for `id` without taking it, for a value whose interval starts later, such
    /// as a block param picked at the first edge reaching it. `excluded` are the registers of
    /// values starting at the same time, `hint` wins unless excluded.
    pub fn choose(
        &mut self,
        id: impl Into<usize>,
        class: Class,
        hint: Option<Reg>,
        excluded: u64,
    ) -> Option<Reg> {
        let id = id.into();
        if let Some(reg) = self.reg(id) {
            return Some(reg);
        }
        let num = match hint {
            Some(hint) if hint.class() == class && excluded & (1u64 << hint.num()) == 0 => {
                hint.num()
            }
            _ => self.pool[class as usize]
                .iter()
                .rev()
                .copied()
                .find(|&num| excluded & (1u64 << num) == 0)?,
        };
        let reg = Reg::new(class, num);
        self.map[id] = Some(reg);
        Some(reg)
    }

    /// Take the register [`Xralloc2::choose`] picked for `id` once its interval starts at `at`.
    pub fn activate(&mut self, id: impl Into<usize>, at: u32, last_use: u32) {
        let reg = self
            .reg(id)
            .expect("activate a value choose picked a register for");
        self.expire(at);
        self.try_take(reg).expect(
            "a chosen register is free at its interval's start, unless a value outlived its block",
        );
        self.active.push((last_use, reg));
    }

    /// Free the registers of values whose last use is at or before `at`.
    pub fn expire(&mut self, at: u32) {
        let Self { active, free, .. } = self;
        active.retain(|&(end, reg)| {
            if end <= at {
                free[reg.class() as usize] |= 1u64 << reg.num();
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

    /// Take `reg` off the free list until [`Xralloc2::give_back`]. `None` if it isn't free.
    pub fn try_take(&mut self, reg: Reg) -> Option<Reg> {
        let bit = 1u64 << reg.num();
        let free = &mut self.free[reg.class() as usize];
        if *free & bit == 0 {
            return None;
        }
        *free &= !bit;
        self.mark_used(reg);
        Some(reg)
    }

    pub fn give_back(&mut self, reg: Reg) {
        self.free[reg.class() as usize] |= 1u64 << reg.num();
    }

    /// Take the most preferred free register of `class`, be it the last or the newest, etc.
    fn pop(&mut self, class: Class) -> Option<u8> {
        let free = &mut self.free[class as usize];
        let num = self.pool[class as usize]
            .iter()
            .rev()
            .copied()
            .find(|&num| *free & (1u64 << num) != 0)?;
        *free &= !(1u64 << num);
        Some(num)
    }

    pub fn used(&self, reg: Reg) -> bool {
        self.used[reg.class() as usize] & (1u64 << reg.num()) != 0
    }

    fn mark_used(&mut self, reg: Reg) {
        self.used[reg.class() as usize] |= 1u64 << reg.num();
    }
}

#[cfg(test)]
mod tests {
    use super::{Class, Reg, Xralloc2};

    /// A value dying at `at` frees its register for the one defined there,
    /// which is what lets `d = l + r` reuse `l`'s register.
    #[test]
    fn dying_value_hands_its_register_to_the_next_def() {
        let mut ra = Xralloc2::default();
        ra.reset(2, &[1, 0], &[]);
        assert_eq!(ra.alloc(0usize, Class::Gpr, 0, 4), Some(Reg::Gpr(0)));
        assert_eq!(ra.alloc(1usize, Class::Gpr, 4, 6), Some(Reg::Gpr(0)));
    }

    /// A chosen register stays free until its value starts, so values dying before then may use
    /// it, and siblings starting together never share one.
    #[test]
    fn chosen_register_is_only_taken_once_its_interval_starts() {
        let mut ra = Xralloc2::default();
        ra.reset(4, &[1, 0], &[]);
        assert_eq!(
            ra.choose(0usize, Class::Gpr, Some(Reg::Gpr(0)), 0),
            Some(Reg::Gpr(0))
        );
        assert_eq!(
            ra.choose(1usize, Class::Gpr, Some(Reg::Gpr(0)), 1),
            Some(Reg::Gpr(1))
        );
        assert_eq!(ra.alloc(2usize, Class::Gpr, 1, 3), Some(Reg::Gpr(0)));
        ra.activate(0usize, 4, 6);
        ra.activate(1usize, 4, 6);
        assert_eq!(ra.alloc(3usize, Class::Gpr, 5, 7), None);
    }

    /// The classes are separate register files: a double and an integer
    /// never compete for a register, and a dying double only frees an fpr.
    #[test]
    fn classes_are_allocated_independently() {
        let mut ra = Xralloc2::default();
        ra.reset(4, &[0], &[0]);
        assert_eq!(ra.alloc(0usize, Class::Gpr, 0, 10), Some(Reg::Gpr(0)));
        assert_eq!(ra.alloc(1usize, Class::Fpr, 0, 2), Some(Reg::Fpr(0)));
        assert_eq!(ra.alloc(2usize, Class::Gpr, 2, 4), None);
        assert_eq!(ra.alloc(3usize, Class::Fpr, 2, 4), Some(Reg::Fpr(0)));
        assert!(ra.used(Reg::Gpr(0)));
        assert!(ra.used(Reg::Fpr(0)));
        assert!(!ra.used(Reg::Fpr(1)));
    }
}
