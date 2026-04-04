use crate::vm;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    Reg(u8),
    Stack(u32),
}

#[derive(Clone, Debug)]
struct Interval {
    v: u32,
    start: u32,
    end: u32,
    reg: Option<u8>,
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
pub struct Ralloc {
    intervals: Vec<Interval>,
    pub map: HashMap<u32, Location>,
}

impl Ralloc {
    pub fn new(live_set: HashMap<u32, (u32, u32)>) -> Self {
        let mut intervals: Vec<Interval> = live_set
            .into_iter()
            .map(|(v, (start, end))| Interval {
                v,
                start,
                end,
                reg: None,
            })
            .collect();

        intervals.sort_by_key(|i| i.start);
        let mut ralloc = Self {
            intervals,
            map: HashMap::new(),
        };
        ralloc.allocate();
        ralloc
    }

    fn allocate(&mut self) {
        let mut active: Vec<Interval> = Vec::new();
        let mut free_regs: Vec<u8> = (0..vm::REGISTER_COUNT as u8).collect();

        for mut interval in self.intervals.clone() {
            // expire old intervals
            active.retain(|i| i.end >= interval.start);

            // reclaim registers from expired intervals / recompute free list each time
            free_regs = (0..vm::REGISTER_COUNT as u8).collect();

            for i in &active {
                if let Some(r) = i.reg {
                    free_regs.retain(|&x| x != r);
                }
            }

            if let Some(reg) = free_regs.pop() {
                interval.reg = Some(reg);
                active.push(interval.clone());
                self.map.insert(interval.v, Location::Reg(reg));
            } else {
                let slot = self.map.len() as u32;
                self.map.insert(interval.v, Location::Stack(slot));
            }
        }
    }
}

#[cfg(test)]
mod regalloc_test {
    use crate::bc::regalloc::{Location, Ralloc};

    #[test]
    fn non_overlapping_reuses_registers() {
        let mut live_set = std::collections::HashMap::new();

        // v0: [0, 2]
        live_set.insert(0, (0, 2));
        // v1: [3, 5]
        live_set.insert(1, (3, 5));

        let ralloc = Ralloc::new(live_set);

        let loc0 = ralloc.map.get(&0).unwrap();
        let loc1 = ralloc.map.get(&1).unwrap();

        match (loc0, loc1) {
            (Location::Reg(r0), Location::Reg(r1)) => {
                // should reuse same register
                assert_eq!(r0, r1);
            }
            _ => panic!("expected both values in registers"),
        }
    }

    #[test]
    fn overlapping_requires_different_registers() {
        let mut live_set = std::collections::HashMap::new();

        // v0: [0, 5]
        live_set.insert(0, (0, 5));
        // v1: [2, 6] overlaps with v0
        live_set.insert(1, (2, 6));

        let ralloc = Ralloc::new(live_set);

        let loc0 = ralloc.map.get(&0).unwrap();
        let loc1 = ralloc.map.get(&1).unwrap();

        match (loc0, loc1) {
            (Location::Reg(r0), Location::Reg(r1)) => {
                assert_ne!(r0, r1, "overlapping intervals must use different registers");
            }
            _ => panic!("expected both values in registers (no spill in this small case)"),
        }
    }

    #[test]
    fn spilling_when_registers_exhausted() {
        let mut live_set = std::collections::HashMap::new();

        let reg_count = crate::vm::REGISTER_COUNT as usize;

        // Create more intervals than registers
        for i in 0..reg_count + 2 {
            live_set.insert(i as u32, (0, 10));
        }

        let ralloc = Ralloc::new(live_set);

        let mut reg_assigned = 0;
        let mut spilled = 0;

        for v in 0..(reg_count + 2) {
            match ralloc.map.get(&(v as u32)).unwrap() {
                Location::Reg(_) => reg_assigned += 1,
                Location::Stack(_) => spilled += 1,
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
        let mut live_set = std::collections::HashMap::new();

        for i in 0..10 {
            live_set.insert(i, (i, i + 1));
        }

        let ralloc = Ralloc::new(live_set);

        for i in 0..10 {
            assert!(
                ralloc.map.contains_key(&i),
                "missing allocation for value {}",
                i
            );
        }
    }

    #[test]
    fn no_register_conflicts() {
        let mut live_set = std::collections::HashMap::new();

        // overlapping chain
        live_set.insert(0, (0, 10));
        live_set.insert(1, (1, 9));
        live_set.insert(2, (2, 8));
        live_set.insert(3, (3, 7));

        let ralloc = Ralloc::new(live_set);

        let mut active: Vec<(u32, u32, u8)> = vec![];

        for (v, (start, end)) in &[(0u32, (0u32, 10u32)), (1, (1, 9)), (2, (2, 8)), (3, (3, 7))] {
            let loc = ralloc.map.get(v).unwrap();
            if let Location::Reg(r) = loc {
                active.push((*start, *end, *r));
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
