use std::alloc::{Allocator, Global};

use crate::Id;

/// What is known about a value, a set of flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Fact(u8);

impl Fact {
    pub const NONZERO: Self = Self(1);

    #[must_use]
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl std::ops::BitOr for Fact {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::fmt::Display for Fact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.contains(Self::NONZERO) {
            f.write_str("nonzero")?;
        }
        Ok(())
    }
}

/// What `opt::facts` proved about the params of a function's blocks, from the branches leading to
/// them. It is the last pass, nothing changes the IR after it, so the backends and the display read
/// these as they are. Empty until it ran: nothing is known and every check stays.
#[derive(Debug, Clone)]
pub struct Facts<A: Allocator = Global> {
    /// Indexed by block id, bit i is the block's i-th param; params past the 64th get no facts and
    /// keep their checks.
    nonzero: Vec<u64, A>,
}

impl Default for Facts {
    fn default() -> Self {
        Self::new_in(Global)
    }
}

impl<A: Allocator> Facts<A> {
    pub fn new_in(alloc: A) -> Self {
        Self {
            nonzero: Vec::new_in(alloc),
        }
    }

    pub fn clear(&mut self) {
        self.nonzero.clear();
    }

    /// `fact` holds for the `index`-th param of `block` inside it.
    pub fn add_param(&mut self, block: Id, index: usize, fact: Fact) {
        if index >= 64 || !fact.contains(Fact::NONZERO) {
            return;
        }
        let i = block.0 as usize;
        if self.nonzero.len() <= i {
            self.nonzero.resize(i + 1, 0);
        }
        self.nonzero[i] |= 1 << index;
    }

    /// What holds for the `index`-th param of `block` inside it.
    #[must_use]
    pub fn of_param(&self, block: Id, index: usize) -> Fact {
        let bits = self.nonzero.get(block.0 as usize).copied().unwrap_or(0);
        if index < 64 && bits & (1 << index) != 0 {
            Fact::NONZERO
        } else {
            Fact::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Fact, Facts};
    use crate::Id;

    #[test]
    fn nothing_holds_until_recorded() {
        let facts = Facts::default();
        assert!(facts.of_param(Id(0), 1).is_empty());
    }

    #[test]
    fn param_facts_stay_in_their_block() {
        let mut facts = Facts::default();
        facts.add_param(Id(2), 1, Fact::NONZERO);
        assert!(facts.of_param(Id(2), 1).contains(Fact::NONZERO));
        assert!(facts.of_param(Id(2), 0).is_empty());
        assert!(facts.of_param(Id(1), 1).is_empty());
    }

    /// Only 64 params fit a block's word, the rest keep their checks.
    #[test]
    fn params_past_the_64th_get_no_facts() {
        let mut facts = Facts::default();
        facts.add_param(Id(0), 64, Fact::NONZERO);
        assert!(facts.of_param(Id(0), 64).is_empty());
    }
}
