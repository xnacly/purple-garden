use std::{alloc::Layout, ffi::c_void};

use crate::{
    Anomaly, BuiltinFn, Vm,
    gc::{AllocType, MAX_ALLOC_SIZE},
};

/// Generates an [`enter_native`]-style entry storing the result with `$store`.
macro_rules! enter_native {
    ($(#[$doc:meta])* $name:ident, $store:literal) => {
        $(#[$doc])*
        ///
        /// # Safety
        ///
        /// `vm` must be a valid, uniquely borrowed pointer to a [`Vm`] and `target` native code
        /// taking its arguments by position, see [`enter_native`].
        #[cfg(target_arch = "x86_64")]
        #[unsafe(naked)]
        pub unsafe extern "C" fn $name(vm: *mut c_void, target: BuiltinFn) {
            std::arch::naked_asm!(
                "push rbx",
                "push rbp",
                "push r12",
                "push r13",
                "push r14",
                "push r15",
                // the return address and six pushes leave rsp 8 off the 16 byte alignment
                // call wants
                "sub rsp, 8",
                "lea rax, [rsp - 8]",
                "mov [rdi + {entry_rsp}], rax",
                // native code keeps the vm in rbx, rdi is free for it
                "mov rbx, rdi",
                "mov r11, rsi",
                // argument i arrives in the i-th gpr and the i-th xmm register, the callee uses
                // the one its type needs
                "mov rsi, [rdi]",
                "mov rdx, [rdi + 8]",
                "mov rcx, [rdi + 16]",
                "mov r8, [rdi + 24]",
                "mov r9, [rdi + 32]",
                "mov r10, [rdi + 40]",
                "movq xmm0, [rdi]",
                "movq xmm1, [rdi + 8]",
                "movq xmm2, [rdi + 16]",
                "movq xmm3, [rdi + 24]",
                "movq xmm4, [rdi + 32]",
                "movq xmm5, [rdi + 40]",
                "call r11",
                // returning and jit_abort both land here
                $store,
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

        /// Native code only exists on x86_64, elsewhere there is nothing to enter.
        ///
        /// # Safety
        ///
        /// `vm` must be a valid, uniquely borrowed pointer to a [`Vm`].
        #[cfg(not(target_arch = "x86_64"))]
        pub unsafe extern "C" fn $name(vm: *mut c_void, target: BuiltinFn) {
            unsafe { target(vm) }
        }
    };
}

enter_native! {
    /// Calls the native function `target` from the interpreter. Native functions take argument i
    /// in `rsi, rdx, rcx, r8, r9, r10`[i] or `xmm0..xmm5`[i] by type and return in `rax`, this
    /// loads `vm.r[0..6]` into both and stores `rax` into `vm.r[0]`, see [`enter_native_double`]
    /// for Double results.
    ///
    /// Saves every callee saved register and records the slot of the call's return address in
    /// [`Vm::native_entry_rsp`], so [`jit_abort`] can leave native code from any depth and both
    /// paths restore the registers.
    enter_native, "mov [rbx], rax"
}

enter_native! {
    /// [`enter_native`] for a native function returning a Double in `xmm0`.
    enter_native_double, "movq [rbx], xmm0"
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
        // a Rust helper calling this may have rbx in use, enter_native stores through it
        "mov rbx, rdi",
        "mov rsp, [rdi + {entry_rsp}]",
        "ret",
        entry_rsp = const crate::NATIVE_ENTRY_RSP_OFFSET,
    )
}

/// Raise a divide-by-zero trap from JIT code and leave native code through
/// [`jit_abort`].
///
/// The JIT cannot write [`Anomaly`] directly because Rust enum layout is not a
/// stable ABI. Callers pass the erased [`Vm`] pointer they received at entry.
/// The VM's `pc` must already point at the trapping bytecode instruction so the
/// diagnostic can be mapped back to source.
///
/// # Safety
///
/// `vm` must be a valid, uniquely borrowed pointer to a [`Vm`].
pub unsafe extern "C" fn jit_trap_div_zero(vm: *mut c_void) -> ! {
    let vm_ref = unsafe { &mut *vm.cast::<Vm>() };
    vm_ref.trap(Anomaly::DivisionByZero { pc: vm_ref.pc });
    unsafe { abort(vm) }
}

/// Raise an assertion failure from JIT code and leave like [`jit_trap_div_zero`].
///
/// # Safety
///
/// `vm` must be a valid, uniquely borrowed pointer to a [`Vm`].
pub unsafe extern "C" fn jit_trap_assert(vm: *mut c_void) -> ! {
    let vm_ref = unsafe { &mut *vm.cast::<Vm>() };
    vm_ref.trap(Anomaly::AssertionFailed { pc: vm_ref.pc });
    unsafe { abort(vm) }
}

/// Raise a stack overflow from JIT code that reached
/// [`Vm::native_stack_limit`] and leave like [`jit_trap_div_zero`].
///
/// # Safety
///
/// `vm` must be a valid, uniquely borrowed pointer to a [`Vm`].
pub unsafe extern "C" fn jit_trap_stack_overflow(vm: *mut c_void) -> ! {
    let vm_ref = unsafe { &mut *vm.cast::<Vm>() };
    vm_ref.trap(Anomaly::StackOverflow { pc: vm_ref.pc });
    unsafe { abort(vm) }
}

/// Call the builtin `f` from JIT code, leaving native code through [`jit_abort`]
/// if it trapped.
///
/// # Safety
///
/// `vm` must be a valid, uniquely borrowed pointer to a [`Vm`] whose `r0..`
/// hold `f`'s arguments.
pub unsafe extern "C" fn jit_sys(vm: *mut c_void, f: BuiltinFn) {
    unsafe { f(vm) };
    if unsafe { &*vm.cast::<Vm>() }.pending_trap.is_some() {
        unsafe { abort(vm) }
    }
}

/// Allocate GC-managed memory from JIT code through [`Vm::try_alloc`].
///
/// `alloc_type` is an [`AllocType`] discriminant, and `size`/`align` are passed
/// to [`Layout::from_size_align`]. Returns the payload pointer. Invalid
/// allocation kinds, invalid layouts, oversized and failed allocations trap and
/// leave native code through [`jit_abort`]. Valid requests use the same
/// collect/no-collect policy as interpreter allocations.
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
    let vm_ref = unsafe { &mut *vm.cast::<Vm>() };
    let ptr = AllocType::from_u8(alloc_type)
        .zip(Layout::from_size_align(size, align).ok())
        .filter(|(_, layout)| layout.size() <= MAX_ALLOC_SIZE)
        .and_then(|(alloc_type, layout)| vm_ref.try_alloc(alloc_type, layout));
    match ptr {
        Some(ptr) => ptr.as_ptr(),
        None => {
            vm_ref.trap(Anomaly::AllocationFailed { pc: vm_ref.pc });
            unsafe { abort(vm) }
        }
    }
}

/// Leaves native code with the trap `vm` holds, see [`jit_abort`].
unsafe fn abort(vm: *mut c_void) -> ! {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        jit_abort(vm)
    }
    #[cfg(not(target_arch = "x86_64"))]
    unreachable!("native code only exists on x86_64, {vm:?} can't be in it")
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

    /// Native code that clobbers every callee saved register but rbx, which holds the vm, and
    /// returns normally.
    #[unsafe(naked)]
    unsafe extern "C" fn clobbers_and_returns(_vm: *mut c_void) {
        std::arch::naked_asm!(
            "mov rbp, -1",
            "mov r12, -1",
            "mov r13, -1",
            "mov r14, -1",
            "mov r15, -1",
            "ret",
        )
    }

    /// Native code that clobbers every callee saved register, the vm in rbx too as a Rust helper
    /// might, and aborts two calls deep, with stack in between.
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
