//! Forward register allocator for the backends:
//!
//! - Registers are handed out in a pass before the backend emits code
//! - A value takes a free register (or a slot) at its definition and keeps it for its whole interval
//! - A register returns to the free list once the interval holding it ends or before the current allocation point
//!
//! For instructions clobbering fixed registers (such as helper calls, or x86 `idiv`)  the live
//! values it would clobber must be copied into free registers around the instruction and again
//! copied back.
//!
//! xralloc2 is target-independent by allowing a pool of registers to be defined per
//! [`Class`]: general purpose registers hold integers and pointers, floating point
//! registers hold doubles.
use std::{
    alloc::{Allocator, Global},
    cmp::Reverse,
    collections::BinaryHeap,
    fmt,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    /// A stack slot, jit indexes via memory offset
    Stack(u16),
    /// A register
    Reg(Reg),
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stack(slot) => write!(f, "s{slot}"),
            Self::Reg(reg) => reg.fmt(f),
        }
    }
}

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

/// The backend's register number, `g` for gprs and `f` for fprs: `g3` is `r3` in bc and `rbx`
/// on x86.
impl fmt::Display for Reg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gpr(num) => write!(f, "g{num}"),
            Self::Fpr(num) => write!(f, "f{num}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Xralloc2<S: Allocator = Global> {
    map: Vec<Option<Location>, S>,
    /// `(last_use, register)` of every value currently holding a register, at most one per
    /// register, so walking them stays cheap however many values live in slots.
    active_regs: Vec<(u32, Reg), S>,
    /// `(last_use, slot)` of every value currently holding a slot, the earliest end first: an
    /// allocation expires them without walking the ones still live.
    active_slots: BinaryHeap<Reverse<(u32, u16)>, S>,
    /// Allocation order per [`Class`], the most preferred register last.
    pool: [Vec<u8, S>; 2],
    /// Bitmask per [`Class`] of the registers currently free.
    free: [u64; 2],
    /// Bitmask per [`Class`] of every register handed out since [`Xralloc2::reset`].
    used: [u64; 2],
    /// Freed slots, reused before a new one is opened.
    free_slots: Vec<u16, S>,
    /// Slots opened since [`Xralloc2::reset`], the frame holds this many.
    slots: u16,
    /// A value needed a slot after all `u16` slots were open, see [`Xralloc2::slots`].
    exhausted: bool,
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
            active_regs: Vec::new_in(scratch.clone()),
            active_slots: BinaryHeap::new_in(scratch.clone()),
            pool: [Vec::new_in(scratch.clone()), Vec::new_in(scratch.clone())],
            free: [0; 2],
            used: [0; 2],
            free_slots: Vec::new_in(scratch),
            slots: 0,
            exhausted: false,
        }
    }

    pub fn reset(&mut self, ids: usize, gprs: &[u8], fprs: &[u8]) {
        self.map.clear();
        self.map.resize(ids, None);
        self.active_regs.clear();
        self.active_slots.clear();
        for (class, regs) in [gprs, fprs].into_iter().enumerate() {
            self.pool[class].clear();
            self.pool[class].extend_from_slice(regs);
            self.free[class] = regs.iter().fold(0, |mask, &num| mask | 1u64 << num);
        }
        self.used = [0; 2];
        self.free_slots.clear();
        self.slots = 0;
        self.exhausted = false;
    }

    pub fn location(&self, id: impl Into<usize>) -> Option<Location> {
        self.map.get(id.into()).copied().flatten()
    }

    /// The register of `id`, `None` if it has none or lives in a slot.
    pub fn reg(&self, id: impl Into<usize>) -> Option<Reg> {
        match self.location(id)? {
            Location::Reg(reg) => Some(reg),
            Location::Stack(_) => None,
        }
    }

    /// Slots opened since [`Xralloc2::reset`], `None` if more values needed one at once than a
    /// `u16` counts: some share a slot, so code using these locations must be discarded.
    pub fn slots(&self) -> Option<u16> {
        (!self.exhausted).then_some(self.slots)
    }

    /// Assign `id` a register of `class`, reusing those of values whose last
    /// use is at or before `at`, or a slot if none is free.
    pub fn alloc(
        &mut self,
        id: impl Into<usize>,
        class: Class,
        at: u32,
        last_use: u32,
    ) -> Location {
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
    ) -> Location {
        self.alloc_inner(id.into(), class, at, last_use, Some(hint))
    }

    fn alloc_inner(
        &mut self,
        id: usize,
        class: Class,
        at: u32,
        last_use: u32,
        hint: Option<Reg>,
    ) -> Location {
        // A value keeps its location for its whole live interval, so if its marked as used and
        // requested again, its the same location
        if let Some(location) = self.location(id) {
            return location;
        }

        // Values whose last use is at or before `at` are dead, their reg should be free again
        //
        // This is what enables reuse of the reg l is in for d in `d = l + r` (if l dead at at)
        self.expire(at);

        // The pool is ordered, so the preferred register (hint) must be at the back and be popped
        // as the allocated register
        let num = match hint {
            Some(hint) if hint.class() == class && self.try_take(hint).is_some() => {
                Some(hint.num())
            }
            _ => self.pop(class),
        };

        let location = match num {
            Some(num) => {
                let reg = Reg::new(class, num);
                self.mark_used(reg);
                self.active_regs.push((last_use, reg));
                Location::Reg(reg)
            }
            None => {
                let slot = self.take_slot();
                self.active_slots.push(Reverse((last_use, slot)));
                Location::Stack(slot)
            }
        };
        crate::trace!(
            "[xralloc2] %v{id} [{at}, {last_use}] -> {location}{}",
            match hint {
                Some(hint) if Location::Reg(hint) == location => " (hinted)".to_string(),
                Some(hint) => format!(" (hint {hint} taken)"),
                None => String::new(),
            }
        );
        self.map[id] = Some(location);
        location
    }

    /// Assign `id` the free register `reg`, where a parameter arrives. `None` if `reg` isn't free.
    pub fn try_alloc_at(&mut self, id: impl Into<usize>, reg: Reg, last_use: u32) -> Option<Reg> {
        self.try_take(reg)?;
        crate::trace!("[xralloc2] param -> {reg}, last use {last_use}");
        self.active_regs.push((last_use, reg));
        self.map[id.into()] = Some(Location::Reg(reg));
        Some(reg)
    }

    /// Free the registers and slots of values whose last use is at or before `at`.
    pub fn expire(&mut self, at: u32) {
        let Self {
            active_regs, free, ..
        } = self;
        active_regs.retain(|&(end, reg)| {
            if end <= at {
                free[reg.class() as usize] |= 1u64 << reg.num();
            }
            end > at
        });
        while let Some(&Reverse((end, slot))) = self.active_slots.peek()
            && end <= at
        {
            self.active_slots.pop();
            self.free_slots.push(slot);
        }
    }

    /// Registers holding values still needed after pos
    pub fn live_after(&self, pos: u32) -> impl Iterator<Item = Reg> + '_ {
        self.active_regs
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

    /// Take a free slot until [`Xralloc2::give_back_slot`], opening one if none is. Once every
    /// `u16` slot is open the allocation is [`Xralloc2::slots`]' `None` and this hands out slot 0.
    pub fn take_slot(&mut self) -> u16 {
        if let Some(slot) = self.free_slots.pop() {
            return slot;
        }
        let Some(slots) = self.slots.checked_add(1) else {
            self.exhausted = true;
            return 0;
        };
        self.slots = slots;
        slots - 1
    }

    pub fn give_back_slot(&mut self, slot: u16) {
        self.free_slots.push(slot);
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
    use super::{Class, Location, Reg, Xralloc2};

    /// A value dying at `at` frees its register for the one defined there,
    /// which is what lets `d = l + r` reuse `l`'s register.
    #[test]
    fn dying_value_hands_its_register_to_the_next_def() {
        let mut ra = Xralloc2::default();
        ra.reset(2, &[1, 0], &[]);
        assert_eq!(
            ra.alloc(0usize, Class::Gpr, 0, 4),
            Location::Reg(Reg::Gpr(0))
        );
        assert_eq!(
            ra.alloc(1usize, Class::Gpr, 4, 6),
            Location::Reg(Reg::Gpr(0))
        );
    }

    /// The classes are separate register files: a double and an integer
    /// never compete for a register, and a dying double only frees an fpr.
    #[test]
    fn classes_are_allocated_independently() {
        let mut ra = Xralloc2::default();
        ra.reset(4, &[0], &[0]);
        assert_eq!(
            ra.alloc(0usize, Class::Gpr, 0, 10),
            Location::Reg(Reg::Gpr(0))
        );
        assert_eq!(
            ra.alloc(1usize, Class::Fpr, 0, 2),
            Location::Reg(Reg::Fpr(0))
        );
        assert_eq!(ra.alloc(2usize, Class::Gpr, 2, 4), Location::Stack(0));
        assert_eq!(
            ra.alloc(3usize, Class::Fpr, 2, 4),
            Location::Reg(Reg::Fpr(0))
        );
        assert!(ra.used(Reg::Gpr(0)));
        assert!(ra.used(Reg::Fpr(0)));
        assert!(!ra.used(Reg::Fpr(1)));
    }

    /// Without a free register a value lives in a slot, which it hands to the next one once it
    /// dies. Only registers count as live across a call.
    #[test]
    fn values_without_a_register_share_slots() {
        let mut ra = Xralloc2::default();
        ra.reset(4, &[0], &[]);
        assert_eq!(
            ra.alloc(0usize, Class::Gpr, 0, 10),
            Location::Reg(Reg::Gpr(0))
        );
        assert_eq!(ra.alloc(1usize, Class::Fpr, 1, 3), Location::Stack(0));
        assert_eq!(ra.alloc(2usize, Class::Gpr, 2, 6), Location::Stack(1));
        assert_eq!(ra.alloc(3usize, Class::Gpr, 3, 6), Location::Stack(0));
        assert_eq!(ra.reg(3usize), None);
        assert_eq!(ra.slots(), Some(2));
        assert_eq!(ra.live_after(4).collect::<Vec<_>>(), [Reg::Gpr(0)]);
    }

    /// Slots are reused in the order their values die, however many are live: freeing them
    /// doesn't walk the live ones, so this runs in n log n.
    #[test]
    fn slots_free_in_the_order_their_values_die() {
        let n = 50_000;
        let mut ra = Xralloc2::default();
        ra.reset(2 * n, &[], &[]);
        // value i lives from i to 2n - i, so the last one defined dies first
        for i in 0..n {
            let at = i as u32;
            assert_eq!(
                ra.alloc(i, Class::Gpr, at, 2 * n as u32 - at),
                Location::Stack(i as u16)
            );
        }
        for i in 0..n {
            let at = (n + 1 + i) as u32;
            assert_eq!(
                ra.alloc(n + i, Class::Gpr, at, 3 * n as u32),
                Location::Stack((n - 1 - i) as u16),
                "value {} died at {at}",
                n - 1 - i
            );
        }
        assert_eq!(ra.slots(), Some(n as u16));
    }

    /// Past the slots a `u16` counts the allocation is unusable, not silently sharing slot 0.
    #[test]
    fn running_out_of_slots_is_reported() {
        let n = usize::from(u16::MAX) + 1;
        let mut ra = Xralloc2::default();
        ra.reset(n, &[], &[]);
        for i in 0..n - 1 {
            ra.alloc(i, Class::Gpr, 0, 1);
        }
        assert_eq!(ra.slots(), Some(u16::MAX));
        assert_eq!(ra.alloc(n - 1, Class::Gpr, 0, 1), Location::Stack(0));
        assert_eq!(ra.slots(), None);
        ra.reset(1, &[], &[]);
        assert_eq!(ra.slots(), Some(0));
    }
}
