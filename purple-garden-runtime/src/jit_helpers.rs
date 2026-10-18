use std::{alloc::Layout, ffi::c_void};

use crate::{
    Anomaly, BuiltinFn, Vm,
    gc::{AllocType, MAX_ALLOC_SIZE},
};

/// Calls the native function `target` from the interpreter. Saves every callee saved register
/// and records the slot of the call's return address in [`Vm::native_entry_rsp`], so
/// [`jit_abort`] can leave native code from any depth and both paths restore the registers.
///
/// # Safety
///
/// `vm` must be a valid, uniquely borrowed pointer to a [`Vm`] and `target` native code
/// following [`BuiltinFn`]'s convention.
#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
pub unsafe extern "C" fn enter_native(vm: *mut c_void, target: BuiltinFn) {
    std::arch::naked_asm!(
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        // the return address and six pushes leave rsp 8 off the 16 byte alignment call wants
        "sub rsp, 8",
        "lea rax, [rsp - 8]",
        "mov [rdi + {entry_rsp}], rax",
        "call rsi",
        // returning and jit_abort both land here
        "add rsp, 8",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
        "ret",
        entry_rsp = const crate::NATIVE_ENTRY_RSP_OFFSET,
    )
}

/// Native code only exists on x86_64, elsewhere there is nothing to abort from.
///
/// # Safety
///
/// `vm` must be a valid, uniquely borrowed pointer to a [`Vm`] and `target` follow
/// [`BuiltinFn`]'s convention.
#[cfg(not(target_arch = "x86_64"))]
pub unsafe extern "C" fn enter_native(vm: *mut c_void, target: BuiltinFn) {
    unsafe { target(vm) }
}

/// Leaves native code from any depth, as if the outermost native function entered through
/// [`enter_native`] returned. Any pending trap stays in [`Vm::pending_trap`].
///
/// # Safety
///
/// `vm` must be the [`Vm`] native code was entered with through [`enter_native`], and every
/// frame left behind must hold nothing that needs dropping.
#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
pub unsafe extern "C" fn jit_abort(vm: *mut c_void) -> ! {
    std::arch::naked_asm!(
        "mov rsp, [rdi + {entry_rsp}]",
        "ret",
        entry_rsp = const crate::NATIVE_ENTRY_RSP_OFFSET,
    )
}

/// Raise a divide-by-zero trap from JIT code. Returns the trap flag native
/// functions hand back in `rax`, so the caller can leave right away.
///
/// The JIT cannot write [`Anomaly`] directly because Rust enum layout is not a
/// stable ABI. Callers pass the erased [`Vm`] pointer they received at entry.
/// The VM's `pc` must already point at the trapping bytecode instruction so the
/// diagnostic can be mapped back to source.
///
/// # Safety
///
/// `vm` must be a valid, uniquely borrowed pointer to a [`Vm`].
pub unsafe extern "C" fn jit_trap_div_zero(vm: *mut c_void) -> u64 {
    let vm = unsafe { &mut *vm.cast::<Vm>() };
    vm.trap(Anomaly::DivisionByZero { pc: vm.pc });
    1
}

/// Raise a stack overflow from JIT code that reached
/// [`Vm::native_stack_limit`]. Returns the trap flag like
/// [`jit_trap_div_zero`].
///
/// # Safety
///
/// `vm` must be a valid, uniquely borrowed pointer to a [`Vm`].
pub unsafe extern "C" fn jit_trap_stack_overflow(vm: *mut c_void) -> u64 {
    let vm = unsafe { &mut *vm.cast::<Vm>() };
    vm.trap(Anomaly::StackOverflow { pc: vm.pc });
    1
}

/// Call the builtin `f` from JIT code, reporting whether it trapped: the
/// native code then returns, so the interpreter surfaces the trap. Natively
/// compiled functions report the flag themselves and are called directly.
///
/// # Safety
///
/// `vm` must be a valid, uniquely borrowed pointer to a [`Vm`] whose `r0..`
/// hold `f`'s arguments.
pub unsafe extern "C" fn jit_sys(vm: *mut c_void, f: BuiltinFn) -> u64 {
    unsafe { f(vm) };
    u64::from(unsafe { &*vm.cast::<Vm>() }.pending_trap.is_some())
}

/// Allocate GC-managed memory from JIT code through [`Vm::try_alloc`].
///
/// `alloc_type` is an [`AllocType`] discriminant, and `size`/`align` are passed
/// to [`Layout::from_size_align`]. Returns the payload pointer on success.
/// Invalid allocation kinds, invalid layouts, or oversized allocations trap and
/// return null; JIT callers must branch on null and return to the native VM
/// entry before using the pointer, so the pending trap can be surfaced. Valid
/// requests use the same collect/no-collect policy as interpreter allocations.
///
/// # Safety
///
/// `vm` must be a valid, uniquely borrowed pointer to a [`Vm`].
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

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    use super::{enter_native, jit_abort};
    use crate::{BuiltinFn, Vm, VmConfig};
    use std::ffi::c_void;

    /// Fills every callee saved register with a marker, enters `target` through
    /// [`enter_native`] and returns 1 if all markers survived.
    #[unsafe(naked)]
    unsafe extern "C" fn keeps_callee_saved(vm: *mut c_void, target: BuiltinFn) -> u64 {
        std::arch::naked_asm!(
            "push rbx",
            "push rbp",
            "push r12",
            "push r13",
            "push r14",
            "push r15",
            "sub rsp, 8",
            "mov rbx, 1",
            "mov rbp, 2",
            "mov r12, 3",
            "mov r13, 4",
            "mov r14, 5",
            "mov r15, 6",
            "call {enter}",
            "xor eax, eax",
            "cmp rbx, 1",
            "jne 2f",
            "cmp rbp, 2",
            "jne 2f",
            "cmp r12, 3",
            "jne 2f",
            "cmp r13, 4",
            "jne 2f",
            "cmp r14, 5",
            "jne 2f",
            "cmp r15, 6",
            "jne 2f",
            "mov eax, 1",
            "2:",
            "add rsp, 8",
            "pop r15",
            "pop r14",
            "pop r13",
            "pop r12",
            "pop rbp",
            "pop rbx",
            "ret",
            enter = sym enter_native,
        )
    }

    /// Native code that clobbers every callee saved register and returns normally.
    #[unsafe(naked)]
    unsafe extern "C" fn clobbers_and_returns(_vm: *mut c_void) {
        std::arch::naked_asm!(
            "mov rbx, -1",
            "mov rbp, -1",
            "mov r12, -1",
            "mov r13, -1",
            "mov r14, -1",
            "mov r15, -1",
            "ret",
        )
    }

    /// Native code that clobbers every callee saved register and aborts two calls deep,
    /// with stack in between.
    #[unsafe(naked)]
    unsafe extern "C" fn clobbers_and_aborts(_vm: *mut c_void) {
        std::arch::naked_asm!(
            "mov rbx, -1",
            "mov rbp, -1",
            "mov r12, -1",
            "mov r13, -1",
            "mov r14, -1",
            "mov r15, -1",
            "sub rsp, 40",
            "call 3f",
            "ud2",
            "3:",
            "push rax",
            "call {abort}",
            "ud2",
            abort = sym jit_abort,
        )
    }

    #[test]
    fn enter_native_restores_callee_saved_on_return() {
        let mut vm = Vm::new(VmConfig::default());
        let vm = (&mut vm as *mut Vm).cast();
        assert_eq!(unsafe { keeps_callee_saved(vm, clobbers_and_returns) }, 1);
    }

    #[test]
    fn jit_abort_returns_to_enter_native_with_callee_saved_intact() {
        let mut vm = Vm::new(VmConfig::default());
        let vm = (&mut vm as *mut Vm).cast();
        assert_eq!(unsafe { keeps_callee_saved(vm, clobbers_and_aborts) }, 1);
        // native code can be entered again after an abort
        assert_eq!(unsafe { keeps_callee_saved(vm, clobbers_and_returns) }, 1);
    }
}
