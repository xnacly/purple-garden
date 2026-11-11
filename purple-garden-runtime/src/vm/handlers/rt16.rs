use super::*;

pub(super) unsafe fn jmp_t<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (cond, target) = op.rt16();
        if r!(vm, cond).as_bool() {
            next!(vm, code, target as usize);
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn jmp_f<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (cond, target) = op.rt16();
        if !r!(vm, cond).as_bool() {
            next!(vm, code, target as usize);
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}
