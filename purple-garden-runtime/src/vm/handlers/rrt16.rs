use super::*;

pub(super) unsafe fn jmp_s_eq<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (lhs, rhs, target) = op.rrt16();
        if r!(vm, lhs).str_eq(r!(vm, rhs)) {
            next!(vm, code, target as usize);
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn jmp_s_ne<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (lhs, rhs, target) = op.rrt16();
        if !r!(vm, lhs).str_eq(r!(vm, rhs)) {
            next!(vm, code, target as usize);
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}
