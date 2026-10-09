//! see https://support.arm.com/documentation/ddi0487/mc/-Part-C-The-AArch64-Instruction-Set?lang=en

use purple_garden_ir as ir;

#[derive(Debug, Clone)]
pub struct Scratch<S>(std::marker::PhantomData<S>);

impl<S> Scratch<S> {
    pub fn new_in(_: S) -> Self {
        Self(std::marker::PhantomData)
    }
}

pub fn compile_func<S: std::alloc::Allocator>(
    _func: &ir::Func<'_>,
    _: &mut Vec<u8, S>,
    _: &[(u32, u32)],
    _: &std::collections::HashMap<ir::Const<'_>, u32>,
    _: &[purple_garden_runtime::Value],
    _: &std::collections::HashMap<ir::Id, purple_garden_runtime::BuiltinFn>,
    _: &mut crate::regalloc::Xralloc2<S>,
    _: &mut Scratch<S>,
) -> Option<()> {
    purple_garden_shared::trace!("[jit::aarch64] skipped: backend scaffold only");
    None
}
