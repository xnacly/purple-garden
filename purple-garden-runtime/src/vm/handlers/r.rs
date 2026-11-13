use super::*;

pub(super) unsafe fn push<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let src = op.r();
        if std::hint::unlikely(!(*(*vm).spilled).push(*r!(vm, src))) {
            trap!(vm, Anomaly::StackOverflow { pc });
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: bc pairs every Pop with an earlier Push
pub(super) unsafe fn pop<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let dst = op.r();
        debug_assert!((*(*vm).spilled).depth() >= size_of::<Value>());
        r_mut!(vm, dst) = (*(*vm).spilled).pop::<Value>();
        next!(vm, code, pc.wrapping_add(1))
    }
}
