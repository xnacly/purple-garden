use super::*;

pub(super) unsafe fn jmp_eq_i<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (lhs, imm, target) = op.ri5t16();
        if r!(vm, lhs).as_int() == imm as i64 {
            next!(vm, code, target as usize);
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn jmp_ne_i<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (lhs, imm, target) = op.ri5t16();
        if r!(vm, lhs).as_int() != imm as i64 {
            next!(vm, code, target as usize);
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}
