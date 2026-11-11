use super::*;

// SAFETY: offset lies within the block base was allocated as
pub(super) unsafe fn store<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (src, base, offset) = op.rru16();
        (r!(vm, base).as_ptr::<u8>().add(offset as usize) as *mut Value).write(*r!(vm, src));
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: offset lies within the block base was allocated as
pub(super) unsafe fn load<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, base, offset) = op.rru16();
        r_mut!(vm, dst) = *(r!(vm, base).as_ptr::<u8>().add(offset as usize) as *const Value);
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: offset lies within the block base was allocated as
pub(super) unsafe fn addr_of<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, base, offset) = op.rru16();
        r_mut!(vm, dst) = Value::from_ptr(r!(vm, base).as_ptr::<u8>().add(offset as usize));
        next!(vm, code, pc.wrapping_add(1))
    }
}
