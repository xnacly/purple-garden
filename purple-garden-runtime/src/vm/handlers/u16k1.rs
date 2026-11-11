use super::*;

// SAFETY: idx present in syscalls, vm not otherwise borrowed
pub(super) unsafe fn native<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let vm = &mut *vm;
        let (idx, returns_double) = op.u16k1();
        vm.pc = pc;
        let enter = if returns_double {
            crate::enter_native_double
        } else {
            crate::enter_native
        };
        enter((vm as V).cast(), *vm.syscalls.add(idx as usize));
        if std::hint::unlikely(vm.pending_trap.is_some()) {
            return 0;
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}
