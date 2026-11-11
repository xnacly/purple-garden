use super::*;

pub(super) unsafe fn mov<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, src) = op.rr();
        r_mut!(vm, dst) = *r!(vm, src);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn push2<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (a, b) = op.rr();
        if std::hint::unlikely(!(*(*vm).spilled).push_all([*r!(vm, a), *r!(vm, b)])) {
            trap!(vm, Anomaly::StackOverflow { pc });
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: bc pairs every Pop with an earlier Push
pub(super) unsafe fn pop2<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (a, b) = op.rr();
        debug_assert!((*(*vm).spilled).depth() >= 2 * size_of::<Value>());
        let [below, top] = (*(*vm).spilled).pop_all::<Value, 2>();
        r_mut!(vm, a) = top;
        r_mut!(vm, b) = below;
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn cast_to_double<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, src) = op.rr();
        r_mut!(vm, dst) = r!(vm, src).int_to_f64();
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn cast_to_int<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, src) = op.rr();
        r_mut!(vm, dst) = r!(vm, src).f64_to_int();
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn cast_to_bool<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, src) = op.rr();
        r_mut!(vm, dst) = r!(vm, src).int_to_bool();
        next!(vm, code, pc.wrapping_add(1))
    }
}
