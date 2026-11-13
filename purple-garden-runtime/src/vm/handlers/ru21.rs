use super::*;

// SAFETY: bc only emits in-bounds registers and globals
pub(super) unsafe fn load_g<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, idx) = op.ru21();
        r_mut!(vm, dst) = *(*vm).globals.as_ptr().add(idx as usize);
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: bc only emits registers below REGISTER_COUNT, and slots inside the frame it
// reserved on the spill stack
pub(super) unsafe fn store_slot<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (src, slot) = op.ru21();
        (*(*vm).spilled).poke(slot as usize, *r!(vm, src));
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: see StoreSlot
pub(super) unsafe fn load_slot<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, slot) = op.ru21();
        r_mut!(vm, dst) = (*(*vm).spilled).peek::<Value>(slot as usize);
        next!(vm, code, pc.wrapping_add(1))
    }
}

pub(super) unsafe fn assert<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (cond, check) = op.ru21();
        trap_if!(
            vm,
            !r!(vm, cond).as_bool(),
            Anomaly::failed(crate::Check::from_u32(check), pc)
        );
        next!(vm, code, pc.wrapping_add(1))
    }
}
