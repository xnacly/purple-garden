//! see https://support.arm.com/documentation/ddi0487/mc/-Part-C-The-AArch64-Instruction-Set?lang=en

use purple_garden_ir as ir;

#[derive(Debug, Default, Clone)]
pub struct Scratch;

pub fn compile_func(
    _func: &ir::Func<'_>,
    _: &mut Vec<u8>,
    _: &[(u32, u32)],
    _: &mut crate::regalloc::Allocator,
    _: &mut Scratch,
) -> Option<()> {
    purple_garden_shared::trace!("[jit::aarch64] skipped: backend scaffold only");
    None
}
