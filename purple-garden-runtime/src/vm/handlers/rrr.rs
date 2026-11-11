use super::*;

pub(super) unsafe fn i_add<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        r_mut!(vm, dst) = Value::from(l + r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn i_sub<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        r_mut!(vm, dst) = Value::from(l - r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn i_mul<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        r_mut!(vm, dst) = Value::from(l * r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn i_div<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        trap_if!(vm, r == 0, Anomaly::DivisionByZero { pc });
        r_mut!(vm, dst) = Value::from(l / r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn i_mod<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        trap_if!(vm, r == 0, Anomaly::DivisionByZero { pc });
        r_mut!(vm, dst) = Value::from(l % r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn i_and<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        r_mut!(vm, dst) = Value::from(l & r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn i_eq<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        r_mut!(vm, dst) = Value::from(l == r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn i_gt<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        r_mut!(vm, dst) = Value::from(l > r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn i_lt<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        r_mut!(vm, dst) = Value::from(l < r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn d_add<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_f64();
        let r = r!(vm, rhs).as_f64();
        r_mut!(vm, dst) = Value::from(l + r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn d_sub<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_f64();
        let r = r!(vm, rhs).as_f64();
        r_mut!(vm, dst) = Value::from(l - r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn d_mul<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_f64();
        let r = r!(vm, rhs).as_f64();
        r_mut!(vm, dst) = Value::from(l * r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn d_div<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_f64();
        let r = r!(vm, rhs).as_f64();
        r_mut!(vm, dst) = Value::from(l / r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn d_gt<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_f64();
        let r = r!(vm, rhs).as_f64();
        r_mut!(vm, dst) = Value::from(l > r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn d_lt<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_f64();
        let r = r!(vm, rhs).as_f64();
        r_mut!(vm, dst) = Value::from(l < r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn b_eq<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_bool();
        let r = r!(vm, rhs).as_bool();
        r_mut!(vm, dst) = Value::from(l == r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn s_eq<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        r_mut!(vm, dst) = Value::from(r!(vm, lhs).str_eq(r!(vm, rhs)));
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn push3<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (a, b, c) = op.rrr();
        if std::hint::unlikely(!(*(*vm).spilled).push_all([*r!(vm, a), *r!(vm, b), *r!(vm, c)])) {
            trap!(vm, Anomaly::StackOverflow { pc });
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: bc pairs every Pop with an earlier Push
pub(super) unsafe fn pop3<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (a, b, c) = op.rrr();
        debug_assert!((*(*vm).spilled).depth() >= 3 * size_of::<Value>());
        let [bottom, below, top] = (*(*vm).spilled).pop_all::<Value, 3>();
        r_mut!(vm, a) = top;
        r_mut!(vm, b) = below;
        r_mut!(vm, c) = bottom;
        next!(vm, code, pc.wrapping_add(1))
    }
}
