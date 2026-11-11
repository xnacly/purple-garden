use super::*;

// SAFETY: bc only emits indices into vm.lookup_tables
pub(super) unsafe fn lookup<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, subject, kind, table) = op.rrk1u15();
        let table = &*(*vm).lookup_tables.as_ptr().add(table as usize);
        let offset = r!(vm, subject).0.wrapping_sub(table.first);
        let slot = match kind {
            SwitchKind::Str => offset / string::ALIGN as u64,
            SwitchKind::Int => offset,
        };
        r_mut!(vm, dst) = *table.values.get(slot as usize).unwrap_or(&table.default);
        next!(vm, code, pc.wrapping_add(1))
    }
}
