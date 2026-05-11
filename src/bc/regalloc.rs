use crate::vm;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    /// Slot has no interval (id is unused, e.g., a tombstoned block's
    /// param). Reading these from `Ralloc::map` is a compiler bug.
    Unassigned,
    Reg(u8),
    Stack,
}

#[derive(Clone, Debug)]
struct Interval {
    v: u32,
    start: u32,
    end: u32,
    reg: Option<u8>,
    /// Soft hint from `ir::Func::arg_hints`: pick this register when
    /// allocating this interval *if* it's currently free. Never blocks
    /// correctness; falls back to standard LIFO if denied.
    preferred: Option<u8>,
}

/// Ralloc is a dumb linear scan register allocator for the purple garden virtual machine.
/// It is neither optimal, nor well done; Its main purpose is to map SSA virtual registers to
/// purple garden virtual machine registers.
///
/// It works by:
/// 1. Sorting virtual register (v) live_set by each start
/// 2. Maintaining a list of currently active intervals
/// 3. For each interval:
///     - Remove active.end lt current.start
///     - Try allocating a register; if avail, otherwise spilled
#[derive(Clone, Debug, Default)]
pub struct Ralloc {
    intervals: Vec<Interval>,
    /// Per-SSA location, indexed by id. Entries for ids without a live
    /// interval stay [`Location::Unassigned`].
    pub map: Vec<Location>,
    /// Running active-set scratch buffer for [`Ralloc::allocate`]. Hoisted
    /// onto the struct so consecutive function compiles reuse the same
    /// allocation.
    active: Vec<Interval>,
}

impl Ralloc {
    /// Refill `intervals`/`map` for a new function and run the linear scan.
    /// Reuses the existing Vec capacities — no allocation when the new
    /// function fits within the previous high-water mark.
    ///
    /// `live_set[id]` is the (def_pos, last_use_pos) for SSA id; entries
    /// with `def_pos == u32::MAX` are unused. `hints[id]` is the optional
    /// preferred register from [`ir::Func::arg_hints_into`].
    pub fn rebuild(&mut self, live_set: &[(u32, u32)], hints: &[Option<u8>]) {
        self.intervals.clear();
        self.intervals.extend(
            live_set
                .iter()
                .enumerate()
                .filter_map(|(v, &(start, end))| {
                    if start == u32::MAX {
                        return None;
                    }
                    let v = v as u32;
                    Some(Interval {
                        v,
                        start,
                        end,
                        reg: None,
                        preferred: hints.get(v as usize).copied().flatten(),
                    })
                }),
        );
        self.intervals.sort_by_key(|i| (i.start, i.v));

        self.map.clear();
        self.map.resize(live_set.len(), Location::Unassigned);

        self.allocate();
    }

    fn allocate(&mut self) {
        // Free-register set as a u64 bitmap: bit `i` set means r{i} is free.
        // Replaces the prior `Vec<u8>`: zero heap allocation, all ops are
        // single-instruction bit-twiddling, hint-availability check is O(1)
        // instead of O(REGISTER_COUNT).
        const _: () = assert!(
            vm::REGISTER_COUNT <= 64,
            "free-reg bitmap fits 64 regs; bump to u128 if REGISTER_COUNT grows"
        );

        // All REGISTER_COUNT low bits set. For REGISTER_COUNT == 64 this
        // is `!0u64`; the shift handles smaller counts cleanly.
        let mut free: u64 = if vm::REGISTER_COUNT == 64 {
            !0u64
        } else {
            (1u64 << vm::REGISTER_COUNT) - 1
        };

        // Split-borrow so we can iterate `intervals` while also mutating
        // `map` and `active` — they're disjoint fields of self.
        let Self {
            intervals,
            map,
            active,
        } = self;
        active.clear();

        for interval in intervals.iter_mut() {
            active.retain(|i: &Interval| {
                if i.end < interval.start {
                    if let Some(r) = i.reg {
                        free |= 1u64 << r;
                    }
                    false
                } else {
                    true
                }
            });

            // Soft hint: take the preferred reg iff its bit is set; else
            // grab the lowest-numbered free reg (preserves the LIFO order
            // the old Vec gave us. top of stack was r0).
            let reg = interval
                .preferred
                .and_then(|p| {
                    let mask = 1u64 << p;
                    if free & mask != 0 {
                        free &= !mask;
                        Some(p)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    let r = free.trailing_zeros();
                    if (r as usize) < vm::REGISTER_COUNT {
                        free &= !(1u64 << r);
                        Some(r as u8)
                    } else {
                        None
                    }
                });

            if let Some(reg) = reg {
                interval.reg = Some(reg);
                active.push(interval.clone());
                map[interval.v as usize] = Location::Reg(reg);
            } else {
                map[interval.v as usize] = Location::Stack;
            }
        }
    }
}

#[cfg(test)]
mod regalloc_test {
    use crate::bc::regalloc::{Location, Ralloc};

    fn build(intervals: &[(u32, (u32, u32))]) -> Vec<(u32, u32)> {
        let max = intervals.iter().map(|(i, _)| *i).max().unwrap_or(0);
        let mut v = vec![(u32::MAX, 0); (max + 1) as usize];
        for (id, range) in intervals {
            v[*id as usize] = *range;
        }
        v
    }

    #[test]
    fn non_overlapping_reuses_registers() {
        let live_set = build(&[(0, (0, 2)), (1, (3, 5))]);

        let mut ralloc = Ralloc::default();
        ralloc.rebuild(&live_set, &[]);

        match (ralloc.map[0], ralloc.map[1]) {
            (Location::Reg(r0), Location::Reg(r1)) => {
                // should reuse same register
                assert_eq!(r0, r1);
            }
            _ => panic!("expected both values in registers"),
        }
    }

    #[test]
    fn overlapping_requires_different_registers() {
        // v0: [0, 5], v1: [2, 6] overlaps with v0
        let live_set = build(&[(0, (0, 5)), (1, (2, 6))]);

        let mut ralloc = Ralloc::default();
        ralloc.rebuild(&live_set, &[]);

        match (ralloc.map[0], ralloc.map[1]) {
            (Location::Reg(r0), Location::Reg(r1)) => {
                assert_ne!(r0, r1, "overlapping intervals must use different registers");
            }
            _ => panic!("expected both values in registers (no spill in this small case)"),
        }
    }

    #[test]
    fn spilling_when_registers_exhausted() {
        let reg_count = crate::vm::REGISTER_COUNT;

        // Create more intervals than registers
        let live_set: Vec<(u32, u32)> = (0..reg_count + 2).map(|_| (0u32, 10u32)).collect();

        let mut ralloc = Ralloc::default();
        ralloc.rebuild(&live_set, &[]);

        let mut reg_assigned = 0;
        let mut spilled = 0;

        for v in 0..(reg_count + 2) {
            match ralloc.map[v] {
                Location::Reg(_) => reg_assigned += 1,
                Location::Stack => spilled += 1,
                Location::Unassigned => panic!("missing allocation for value {v}"),
            }
        }

        assert!(spilled > 0, "expected some values to spill");
        assert_eq!(
            reg_assigned, reg_count,
            "registers should be fully utilized"
        );
    }

    #[test]
    fn all_values_assigned() {
        let live_set: Vec<(u32, u32)> = (0..10u32).map(|i| (i, i + 1)).collect();

        let mut ralloc = Ralloc::default();
        ralloc.rebuild(&live_set, &[]);

        for i in 0..10 {
            assert!(
                !matches!(ralloc.map[i], Location::Unassigned),
                "missing allocation for value {i}"
            );
        }
    }

    #[test]
    fn no_register_conflicts() {
        // overlapping chain
        let live_set = build(&[(0, (0, 10)), (1, (1, 9)), (2, (2, 8)), (3, (3, 7))]);

        let mut ralloc = Ralloc::default();
        ralloc.rebuild(&live_set, &[]);

        let mut active: Vec<(u32, u32, u8)> = vec![];

        for (v, (start, end)) in &[(0u32, (0u32, 10u32)), (1, (1, 9)), (2, (2, 8)), (3, (3, 7))] {
            if let Location::Reg(r) = ralloc.map[*v as usize] {
                active.push((*start, *end, r));
            }
        }

        // check overlaps don't share registers
        for i in 0..active.len() {
            for j in i + 1..active.len() {
                let (s1, e1, r1) = active[i];
                let (s2, e2, r2) = active[j];

                let overlap = !(e1 < s2 || e2 < s1);
                if overlap {
                    assert_ne!(r1, r2, "overlapping intervals share a register");
                }
            }
        }
    }
}
