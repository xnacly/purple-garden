use crate::vm;

/// Works by keeping a list of currently free registers, pushing is equivalent to freeing a
/// register, while popping is an allocation. Loosely based on [Poletto, Massimiliano, and Vivek
/// Sarkar. "Linear scan register allocation."](https://c9x.me/compile/bib/linearscan.pdf)
#[derive(Debug)]
pub struct RegisterAllocator {
    free: Vec<u8>,
    marks: Vec<Vec<u8>>,
}

impl RegisterAllocator {
    pub fn new() -> Self {
        Self {
            // reversing the register count makes the lower registers "hot"
            free: (0..vm::REGISTER_COUNT as u8).rev().collect(),
            marks: Vec::with_capacity(16),
        }
    }

    pub fn alloc(&mut self) -> u8 {
        #[cfg(feature = "trace")]
        println!("RegisterAllocator::alloc(r{:?})", self.free.last().unwrap());
        self.free.pop().unwrap_or_else(|| {
            panic!("RegisterAllocator: out of registers, do open a bug report please")
        })
    }

    /// allocates [n] registers, returns the number to the last allocated register, thus it can be
    /// used to allocate a contiguous window into the register file:
    ///
    ///     let mut reg = RegisterAllocator::new();
    ///     const REG = 8;
    ///     let last_reg = reg.alloc_n(REG);
    ///     let first_reg = last_reg-(REG-1);
    pub fn alloc_n(&mut self, n: u8) -> u8 {
        assert!(
            n as usize <= self.free.len(),
            "RegisterAllocator: not enough free registers"
        );

        let mut last_reg = 0;
        for _ in 0..n {
            last_reg = self.free.pop().unwrap();
        }

        #[cfg(feature = "trace")]
        println!(
            "RegisterAllocator::alloc(r{}..r{})",
            last_reg,
            last_reg + 1 - n
        );

        last_reg
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

    pub fn mark(&mut self) {
        self.marks.push(self.free.clone());
    }

    pub fn reset_to_last_mark(&mut self) {
        if let Some(stack) = self.marks.pop() {
            self.free = stack;
        } else {
            panic!("No mark to reset to");
        }
    }
}
