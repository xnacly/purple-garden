use std::alloc::Allocator;

use crate::Id;

/// What `opt::facts` proved about the params of a function's blocks, from the branches leading to
/// them. It is the last pass, nothing changes the IR after it, so the backends and the display read
/// these as they are. Indexed by block id, bit i is the block's i-th param; params past the 64th
/// get no facts and keep their checks.
#[derive(Debug, Clone)]
pub struct Facts<A: Allocator> {
    nonzero: Vec<u64, A>,
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

    pub fn set_nonzero(&mut self, block: Id, param: usize) {
        if param >= 64 {
            return;
        }
        let b = block.0 as usize;
        if self.nonzero.len() <= b {
            self.nonzero.resize(b + 1, 0);
        }
        self.nonzero[b] |= 1 << param;
    }

    #[must_use]
    pub fn nonzero(&self, block: Id, param: usize) -> bool {
        param < 64
            && self
                .nonzero
                .get(block.0 as usize)
                .is_some_and(|bits| bits >> param & 1 == 1)
    }
}
