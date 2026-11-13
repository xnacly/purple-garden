//! see https://support.arm.com/documentation/ddi0487/mc/-Part-C-The-AArch64-Instruction-Set?lang=en

use std::alloc::Allocator;

use purple_garden_ir as ir;

#[derive(Debug, Clone)]
pub struct Scratch<S>(std::marker::PhantomData<S>);

impl<S> Scratch<S> {
    pub fn new_in(_: S) -> Self {
        Self(std::marker::PhantomData)
    }

    pub fn tables(&self) -> &[std::ops::Range<usize>] {
        &[]
    }

    pub fn rows(&self) -> &[(u32, usize)] {
        &[]
    }
}

pub fn compile_func<'ir, F: Allocator, S: Allocator, N: Allocator>(
    _func: &ir::Func<'ir, F>,
    _: &mut Vec<u8, S>,
    _: &[(u32, u32)],
    _: &std::collections::HashMap<ir::Const<'_>, u32>,
    _: &[purple_garden_runtime::Value],
    _: &crate::Natives<N>,
    _: usize,
    _: &mut crate::xralloc2::Xralloc2<S>,
    _: &mut Scratch<S>,
) -> Result<(), crate::Unsupported<'ir>> {
    Err(crate::Unsupported::Scaffold)
}
