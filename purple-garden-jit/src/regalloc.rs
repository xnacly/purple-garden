//! Minimal linear-scan register allocator for the JIT.
//!
//! Reuses the IR liveness intervals (the same analysis the bytecode backend
//! consumes) and assigns each SSA value a physical register from one of two
//! classes. A value whose live range spans a call must survive it, so it takes a
//! callee-saved register; everything else takes the cheaper caller-saved class.
//! Values that don't fit spill to `Location::Stack` and the lowering bails
//! (falls back to bytecode).
//!
//! Target-independent: the caller passes both register pools, so a future
//! aarch64 backend reuses it unchanged.

use purple_garden_ir as ir;

/// The two register classes the allocator draws from.
pub struct RegClasses<'a> {
    /// Clobbered by a call; cheapest, for values not live across one.
    pub caller: &'a [u8],
    /// Preserved across a call; for values that span one. The lowering saves the
    /// ones it used in its prologue.
    pub callee: &'a [u8],
}

/// `[def, last_use]` is live across a call iff a call sits strictly inside it.
fn spans_call(def: u32, last_use: u32, call_sites: &[u32]) -> bool {
    call_sites.iter().any(|&c| def < c && c < last_use)
}

/// Linear scan over `liveness` (`(def_pos, last_use_pos)` per SSA id; a
/// `u32::MAX` start marks an unused id). `call_sites` are call positions in the
/// same coordinate space. A value spanning a call takes a `classes.callee`
/// register, others `classes.caller`; an exhausted class spills that value.
#[derive(Debug, Default, Clone)]
pub struct Allocator {
    map: Vec<ir::Location>,
    order: Vec<usize>,
    caller_free: Vec<u8>,
    callee_free: Vec<u8>,
    active: Vec<(u32, u8, bool)>, // (last_use, reg, from_callee)
}

impl Allocator {
    pub fn rebuild(
        &mut self,
        liveness: &[(u32, u32)],
        call_sites: &[u32],
        classes: RegClasses<'_>,
    ) -> &[ir::Location] {
        self.map.clear();
        self.map.resize(liveness.len(), ir::Location::Unassigned);

        self.order.clear();
        let mut last_start = 0;
        let mut already_sorted = true;
        for (v, &(start, _)) in liveness.iter().enumerate() {
            if start == u32::MAX {
                continue;
            }
            already_sorted &= self.order.is_empty() || start >= last_start;
            last_start = start;
            self.order.push(v);
        }
        if !already_sorted {
            self.order.sort_by_key(|&v| (liveness[v].0, v));
        }

        self.caller_free.clear();
        self.caller_free
            .extend(classes.caller.iter().rev().copied());
        self.callee_free.clear();
        self.callee_free
            .extend(classes.callee.iter().rev().copied());
        self.active.clear();

        for &v in &self.order {
            let (start, end) = liveness[v];
            self.active.retain(|&(last_use, reg, from_callee)| {
                if last_use <= start {
                    if from_callee {
                        self.callee_free.push(reg);
                    } else {
                        self.caller_free.push(reg);
                    }
                    false
                } else {
                    true
                }
            });

            let from_callee = spans_call(start, end, call_sites);
            let free = if from_callee {
                &mut self.callee_free
            } else {
                &mut self.caller_free
            };
            self.map[v] = match free.pop() {
                Some(reg) => {
                    self.active.push((end, reg, from_callee));
                    ir::Location::Reg(reg)
                }
                None => ir::Location::Stack,
            };
        }

        &self.map
    }
}

#[cfg(test)]
mod tests {
    use super::{Allocator, RegClasses};
    use purple_garden_ir::Location;

    const CALLER: &[u8] = &[0, 1];
    const CALLEE: &[u8] = &[3, 12];

    fn alloc(liveness: &[(u32, u32)], call_sites: &[u32]) -> Vec<Location> {
        Allocator::default()
            .rebuild(
                liveness,
                call_sites,
                RegClasses {
                    caller: CALLER,
                    callee: CALLEE,
                },
            )
            .to_vec()
    }

    fn spills(map: &[Location]) -> usize {
        map.iter().filter(|l| matches!(l, Location::Stack)).count()
    }

    #[test]
    fn disjoint_intervals_reuse_a_register() {
        let map = alloc(&[(0, 2), (3, 5)], &[]);
        assert_eq!(map[0], Location::Reg(0));
        assert_eq!(map[1], Location::Reg(0));
    }

    #[test]
    fn overlapping_intervals_get_distinct_registers() {
        let map = alloc(&[(0, 5), (2, 6)], &[]);
        assert_eq!(map[0], Location::Reg(0));
        assert_eq!(map[1], Location::Reg(1));
    }

    #[test]
    fn out_of_order_intervals_are_sorted_by_start() {
        let map = alloc(&[(5, 8), (0, 10)], &[]);
        assert_eq!(map[1], Location::Reg(0));
        assert_eq!(map[0], Location::Reg(1));
    }

    #[test]
    fn exhausted_caller_pool_spills() {
        assert_eq!(spills(&alloc(&[(0, 9), (1, 9), (2, 9)], &[])), 1);
    }

    #[test]
    fn unused_ids_stay_unassigned() {
        let map = alloc(&[(u32::MAX, 0), (0, 1)], &[]);
        assert_eq!(map[0], Location::Unassigned);
        assert_eq!(map[1], Location::Reg(0));
    }

    #[test]
    fn value_spanning_a_call_takes_a_callee_register() {
        // call at pos 5 sits inside [0,10], so v0 must survive it.
        assert_eq!(alloc(&[(0, 10)], &[5])[0], Location::Reg(3));
    }

    #[test]
    fn value_not_crossing_a_call_stays_caller_saved() {
        // call at pos 20 is after v0's last use, so it doesn't span it.
        assert_eq!(alloc(&[(0, 10)], &[20])[0], Location::Reg(0));
    }

    #[test]
    fn cross_call_value_spills_when_callee_class_is_full() {
        // all three span the call but only one callee reg exists: two spill,
        // with no fallback to the caller class.
        let map = Allocator::default()
            .rebuild(
                &[(0, 10), (0, 10), (0, 10)],
                &[5],
                RegClasses {
                    caller: CALLER,
                    callee: &[3],
                },
            )
            .to_vec();
        assert_eq!(spills(&map), 2);
    }
}
