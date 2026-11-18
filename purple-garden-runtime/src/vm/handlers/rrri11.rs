use super::*;

pub(super) unsafe fn idx<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, target, idx, stride) = op.rrri11();
        let idx = r!(vm, idx).as_int();
        let ptr = r!(vm, target).as_ptr::<u8>();

        // idx has to uphold 0<=idx<len(src), where len(src) is ptr+0, see layout of arrays
        trap_if!(
            vm,
            idx as u64 >= *(ptr as *const u64),
            Anomaly::IndexOutOfBounds { pc }
        );

        let offset = 8 + idx as usize * stride as usize;
        r_mut!(vm, dst) = Value::from_ptr(ptr.add(offset));
        next!(vm, code, pc.wrapping_add(1))
    }
}
