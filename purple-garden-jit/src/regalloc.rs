//! Minimal linear-scan register allocator for the JIT.
//!
//! Reuses the IR liveness intervals (`ir::Func::live_set_into`, the same
//! analysis the bytecode backend consumes) and assigns each SSA value a physical
//! register from a target-provided pool. Values that don't fit spill to
//! [`ir::Location::Stack`]; the lowering bails on spills for now.
//!
//! The scan itself is target-independent; the caller passes the pool of
//! allocatable physical register numbers (e.g. the x86-64 GPRs), so a future
//! aarch64 backend can reuse it unchanged.

use purple_garden_ir as ir;

// TODO: this is currently very uncomplicated, as soon as the jit supports branches, calls and other
// stuff, this alloc will also require hints, bitmaps for free regs, etc, once that happens this
// should be a unified purple-garden-regalloc crate

/// Linear scan over `liveness` (`(def_pos, last_use_pos)` per SSA id; a `u32::MAX`
/// start marks an unused id). `pool` is the ordered set of allocatable physical
/// register numbers. Returns a per-SSA-id map.
#[must_use]
pub fn allocate(liveness: &[(u32, u32)], pool: &[u8]) -> Vec<ir::Location> {
    let mut map = vec![ir::Location::Unassigned; liveness.len()];

    let mut order: Vec<usize> = (0..liveness.len())
        .filter(|&v| liveness[v].0 != u32::MAX)
        .collect();
    order.sort_by_key(|&v| (liveness[v].0, v));

    // `free` is a stack of available physical regs; reversed so we hand out the
    // pool in its given order (pool[0] first).
    let mut free: Vec<u8> = pool.iter().rev().copied().collect();
    let mut active: Vec<(u32, u8)> = Vec::new();

    for v in order {
        let (start, end) = liveness[v];
        // Expire intervals that died at or before this one's def. End-inclusive
        // (`<=`) mirrors the bytecode allocator: liveness encodes each position
        // as early+late, so a dying value and a newborn one can share a reg.
        active.retain(|&(last_use, reg)| {
            if last_use <= start {
                free.push(reg);
                false
            } else {
                true
            }
        });

        if let Some(reg) = free.pop() {
            map[v] = ir::Location::Reg(reg);
            active.push((end, reg));
        } else {
            map[v] = ir::Location::Stack;
        }
    }

    map
}

#[cfg(test)]
mod tests {
    use super::allocate;
    use purple_garden_ir::Location;

    #[test]
    fn disjoint_intervals_reuse_a_register() {
        // v0 lives [0,2], v1 lives [3,5]: disjoint, so v1 reuses v0's reg.
        let map = allocate(&[(0, 2), (3, 5)], &[0, 1]);
        assert_eq!(map[0], Location::Reg(0));
        assert_eq!(map[1], Location::Reg(0));
    }

    #[test]
    fn overlapping_intervals_get_distinct_registers() {
        let map = allocate(&[(0, 5), (2, 6)], &[0, 1]);
        assert_eq!(map[0], Location::Reg(0));
        assert_eq!(map[1], Location::Reg(1));
    }

    #[test]
    fn exhausted_pool_spills() {
        // three values all live at once, pool of two: one spills.
        let map = allocate(&[(0, 9), (1, 9), (2, 9)], &[0, 1]);
        let spilled = map.iter().filter(|l| matches!(l, Location::Stack)).count();
        assert_eq!(spilled, 1);
    }

    #[test]
    fn unused_ids_stay_unassigned() {
        let map = allocate(&[(u32::MAX, 0), (0, 1)], &[0, 1]);
        assert_eq!(map[0], Location::Unassigned);
        assert_eq!(map[1], Location::Reg(0));
    }
}
