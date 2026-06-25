use std::{alloc::Layout, ffi::c_void};

use crate::{
    Anomaly, Vm,
    gc::{AllocType, MAX_ALLOC_SIZE},
};

/// Raise a divide-by-zero trap from JIT code.
///
/// The JIT cannot write [`Anomaly`] directly because Rust enum layout is not a
/// stable ABI. Callers pass the erased [`Vm`] pointer they received at entry.
/// The VM's `pc` must already point at the trapping bytecode instruction so the
/// diagnostic can be mapped back to source.
pub unsafe extern "C" fn jit_trap_div_zero(vm: *mut c_void) {
    let vm = unsafe { &mut *vm.cast::<Vm>() };
    vm.trap(Anomaly::DivisionByZero { pc: vm.pc });
}

/// Allocate GC-managed memory from JIT code through [`Vm::try_alloc`].
///
/// `alloc_type` is an [`AllocType`] discriminant, and `size`/`align` are passed
/// to [`Layout::from_size_align`]. Returns the payload pointer on success.
/// Invalid allocation kinds, invalid layouts, or oversized allocations trap and
/// return null; JIT callers must branch on null and return to the native VM
/// entry before using the pointer, so the pending trap can be surfaced. Valid
/// requests use the same collect/no-collect policy as interpreter allocations.
pub unsafe extern "C" fn jit_alloc(
    vm: *mut c_void,
    alloc_type: u8,
    size: usize,
    align: usize,
) -> *mut u8 {
    let vm = unsafe { &mut *vm.cast::<Vm>() };

    let Some(alloc_type) = AllocType::from_u8(alloc_type) else {
        vm.trap(Anomaly::AllocationFailed { pc: vm.pc });
        return std::ptr::null_mut();
    };
    let Ok(layout) = Layout::from_size_align(size, align) else {
        vm.trap(Anomaly::AllocationFailed { pc: vm.pc });
        return std::ptr::null_mut();
    };
    if layout.size() > MAX_ALLOC_SIZE {
        vm.trap(Anomaly::AllocationFailed { pc: vm.pc });
        return std::ptr::null_mut();
    }

    match vm.try_alloc(alloc_type, layout) {
        Some(ptr) => ptr.as_ptr(),
        None => {
            vm.trap(Anomaly::AllocationFailed { pc: vm.pc });
            std::ptr::null_mut()
        }
    }
}
