use super::*;

pub(super) unsafe fn i_add_i<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l + imm as i64);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn i_sub_i<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l - imm as i64);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn i_mul_i<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l * imm as i64);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn i_div_i<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let imm = imm as i64;
        trap_if!(vm, imm == 0, Anomaly::DivisionByZero { pc });
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l / imm);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn i_mod_i<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let imm = imm as i64;
        trap_if!(vm, imm == 0, Anomaly::DivisionByZero { pc });
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l % imm);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn i_and_i<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l & imm as i64);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn i_eq_i<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l == imm as i64);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn i_gt_i<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l > imm as i64);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn i_lt_i<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l < imm as i64);
        next!(vm, code, pc.wrapping_add(1))
    }
}
