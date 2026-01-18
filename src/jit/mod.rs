use crate::ir::IrNode;

pub struct ExecBuffer {
    ptr: *mut u8,
    size: usize,
}

/// The baseline just in time compiler targetting x86 and aarch64.
pub struct Bjit<'jit> {
    buf: &'jit mut ExecBuffer,
}

impl<'jit> Bjit<'jit> {
    pub fn new(buf: &'jit mut ExecBuffer) -> Self {
        Bjit { buf }
    }

    pub fn aarch64_from(ir: &[IrNode]) {}

    /// https://c9x.me/x86/
    pub fn x86_from(ir: &[IrNode]) {}
}
