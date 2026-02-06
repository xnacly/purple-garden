use crate::{
    asm::{aarch64, x86},
    ir, vm,
};

mod buf;
mod consts;

/// The baseline just in time compiler targetting x86_64 (SysV) and aarch64 (macos/linux arm64).
pub struct Bjit<'jit> {
    pub buf: &'jit mut buf::ExecBuffer,
}

impl<'jit> Bjit<'jit> {
    pub fn new(buf: &'jit mut buf::ExecBuffer) -> Self {
        Bjit { buf }
    }

    pub fn from(ir: &[ir::Func]) {
        #[cfg(target_arch = "x86_64")]
        {
            Self::to_x86(ir);
        }

        #[cfg(target_arch = "aarch64")]
        {
            Self::to_aarch64(ir);
        }
    }

    /// https://c9x.me/x86/
    pub fn to_x86(ir: &[ir::Func]) {
        // TODO: lower ir to x86 instructions:
        //
        // - register allocator according to SysV ABI
        // - encode ops as x86_64 instructions, add asm::x86 module
        unimplemented!()
    }

    /// https://www.cs.swarthmore.edu/~kwebb/cs31/resources/ARM64_Cheat_Sheet.pdf
    pub fn to_aarch64(ir: &[ir::Func]) {
        // TODO: lower ir to aarch64 instructions (i dont know anything about em, they do seem to
        // differ a lot from armv7):
        //
        // - register allocator according to amr64 ABI
        // - encode ops as aarch64 instructions, add asm::aarch64 module
        unimplemented!()
    }
}
