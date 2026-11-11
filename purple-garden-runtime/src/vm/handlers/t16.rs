use super::*;

pub(super) unsafe fn jmp<const B: bool>(
    vm: V,
    code: *const Packed,
    _pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let target = op.t16();
        next!(vm, code, target as usize)
    }
}
