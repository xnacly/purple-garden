use purple_garden_ir as ir;

pub fn compile_func(
    _func: &ir::Func<'_>,
    _: &mut Vec<u8>,
    _: &[(u32, u32)],
    _: &mut crate::regalloc::Allocator,
) -> Option<()> {
    purple_garden_shared::trace!("[jit::aarch64] skipped: backend scaffold only");
    None
}
