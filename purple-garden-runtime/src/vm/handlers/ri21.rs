use super::*;

pub(super) unsafe fn load_i<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, value) = op.ri21();
        r_mut!(vm, dst) = Value::from(value as i64);
        next!(vm, code, pc.wrapping_add(1))
    }
}
