use crate::vm;

/// The simplest register allocation strategy I could think of, will probably explode on heavy
/// usage... Works by keeping a list of currently free registers
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
    }
}

impl Drop for RegisterAllocator {
    fn drop(&mut self) {
        assert!(
            self.free.len() == vm::REGISTER_COUNT,
            "RegisterAllocator: not all registers freed at exit, register leak, this is a compiler bug, please open a bug report"
        )
    }
}
