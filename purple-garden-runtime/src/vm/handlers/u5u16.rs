use super::*;

// SAFETY: bc only emits Sys indices present in syscalls
pub(super) unsafe fn sys<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let vm = &mut *vm;
        let (argc, idx) = op.u5u16();
        #[cfg(not(debug_assertions))]
        let _ = argc;
        #[cfg(debug_assertions)]
        let pre_sys: [Value; REGISTER_COUNT] = vm.r;

        // Publish the current pc before the call: a syscall that
        // traps reads vm.pc to locate itself, but the loop only
        // writes vm.pc on exit, so without this the trap would
        // carry a stale pc and render at the wrong source span.
        vm.pc = pc;
        (*vm.syscalls.add(idx as usize))((vm as V).cast());

        #[cfg(debug_assertions)]
        for (i, pre) in pre_sys.iter().enumerate().skip(usize::from(argc.max(1))) {
            debug_assert_eq!(
                pre.0,
                vm.r[i].0,
                "syscall idx={idx} wrote r{i}; convention only permits writes to r0..r{}",
                argc.max(1) - 1
            );
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}
