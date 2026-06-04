use std::fmt;

use purple_garden_ir as ir;

#[derive(Debug, Clone, Copy)]
pub enum Insn {}

impl Insn {
    pub fn encode(self, _: &mut Vec<u8>) {
        match self {}
    }
}

impl fmt::Display for Insn {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {}
    }
}

pub fn compile_func(
    _func: &ir::Func<'_>,
    _: &mut Vec<Insn>,
    _: &[(u32, u32)],
    _: &mut crate::regalloc::Allocator,
) -> Option<()> {
    purple_garden_shared::trace!("[jit::aarch64] skipped: backend scaffold only");
    None
}
