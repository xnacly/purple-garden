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
#[must_use]
pub fn allocate(
    liveness: &[(u32, u32)],
    call_sites: &[u32],
    classes: RegClasses,
) -> Vec<ir::Location> {
    let mut map = vec![ir::Location::Unassigned; liveness.len()];

    let mut order: Vec<usize> = (0..liveness.len())
        .filter(|&v| liveness[v].0 != u32::MAX)
        .collect();
    order.sort_by_key(|&v| (liveness[v].0, v));

    // Per-class free stacks, reversed so each pool is handed out in given order.
    let mut caller_free: Vec<u8> = classes.caller.iter().rev().copied().collect();
    let mut callee_free: Vec<u8> = classes.callee.iter().rev().copied().collect();
    let mut active: Vec<(u32, u8, bool)> = Vec::new(); // (last_use, reg, from_callee)

    for v in order {
        let (start, end) = liveness[v];
        active.retain(|&(last_use, reg, from_callee)| {
            if last_use <= start {
                if from_callee {
                    callee_free.push(reg);
                } else {
                    caller_free.push(reg);
                }
                false
            } else {
                true
            }
        });

        let from_callee = spans_call(start, end, call_sites);
        let free = if from_callee {
            &mut callee_free
        } else {
            &mut caller_free
        };
        map[v] = match free.pop() {
            Some(reg) => {
                active.push((end, reg, from_callee));
                ir::Location::Reg(reg)
            }
            None => ir::Location::Stack,
        };
    }

    map
}

#[cfg(test)]
mod tests {
    use super::{RegClasses, allocate};
    use purple_garden_ir::Location;

    const CALLER: &[u8] = &[0, 1];
    const CALLEE: &[u8] = &[3, 12];

    fn alloc(liveness: &[(u32, u32)], call_sites: &[u32]) -> Vec<Location> {
        allocate(
            liveness,
            call_sites,
            RegClasses {
                caller: CALLER,
                callee: CALLEE,
            },
        )
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
        let map = allocate(
            &[(0, 10), (0, 10), (0, 10)],
            &[5],
            RegClasses {
                caller: CALLER,
                callee: &[3],
            },
        );
        assert_eq!(spills(&map), 2);
    }
}
