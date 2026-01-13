use crate::vm;

/// Works by keeping a list of currently free registers, pushing is equivalent to freeing a
/// register, while popping is an allocation. Loosely based on [Poletto, Massimiliano, and Vivek
/// Sarkar. "Linear scan register allocation."](https://c9x.me/compile/bib/linearscan.pdf)
#[derive(Debug)]
pub struct RegisterAllocator {
    free: Vec<u8>,
}

impl RegisterAllocator {
    pub fn new() -> Self {
        Self {
            // reversing the register count makes the lower registers "hot"
            free: (0..vm::REGISTER_COUNT as u8).rev().collect(),
        }
    }

    pub fn alloc(&mut self) -> u8 {
        #[cfg(feature = "trace")]
        println!("RegisterAllocator::alloc(r{:?})", self.free.last().unwrap());
        self.free.pop().unwrap_or_else(|| {
            panic!("RegisterAllocator: out of registers, do open a bug report please")
        })
    }

    pub fn free(&mut self, r: u8) {
        #[cfg(feature = "trace")]
        println!("RegisterAllocator::free(r{r})");
        self.free.push(r);
        assert!(
            !(self.free.len() > vm::REGISTER_COUNT),
            "Freed one too many registers"
        );
    }
}
