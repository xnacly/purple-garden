use super::*;

// SAFETY: size and align come from a Layout in lowering
pub(super) unsafe fn alloc<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let vm = &mut *vm;
        let (dst, kind, align, size) = op.rk2u3u16();
        let ptr = vm.alloc(
            kind,
            Layout::from_size_align_unchecked(size as usize, align as usize),
        );
        r_mut!(vm, dst) = Value::from_ptr(ptr.as_ptr());
        next!(vm, code, pc.wrapping_add(1))
    }
}
