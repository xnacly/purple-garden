use super::*;

pub(super) unsafe fn tail<const B: bool>(
    vm: V,
    code: *const Packed,
    _pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let vm = &mut *vm;
        let func = op.u26();
        if B {
            vm.backtrace.push(func as usize);
        }
        next!(vm, code, func as usize)
    }
}

pub(super) unsafe fn call<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let vm = &mut *vm;
        let func = op.u26();
        if B {
            vm.backtrace.push(func as usize);
        }

        let depth = vm.frame_depth;
        if std::hint::unlikely(depth == vm.frames.len()) {
            trap!(vm, Anomaly::StackOverflow { pc });
        }

        // SAFETY: depth < frames.len() checked above
        *vm.frames.get_unchecked_mut(depth) = CallFrame {
            return_to: pc,
            #[cfg(debug_assertions)]
            spilled_depth: (*vm.spilled).depth(),
        };
        vm.frame_depth = depth + 1;

        next!(vm, code, func as usize)
    }
}
