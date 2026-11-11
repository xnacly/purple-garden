use super::*;

// SAFETY: bc only emits indices into vm.switch_tables
pub(super) unsafe fn switch<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (subject, kind, table) = op.rk1u20();
        let table = &*(*vm).switch_tables.as_ptr().add(table as usize);
        // Strings are interned, equal strings share one address. string key is its
        // address in the string pool, an int key its value. Wrapping keeps a subject
        // below `first` huge, so the bounds check below rejects it along with those
        // past the end.
        let offset = r!(vm, subject).0.wrapping_sub(table.first);
        // Pool strings sit ALIGN bytes apart, one slot each.
        let slot = match kind {
            SwitchKind::Str => offset / string::ALIGN as u64,
            SwitchKind::Int => offset,
        };
        // Slots between keys hold the default's pc; outside the
        // table, fall through to the `jmp default` after this op.
        if let Some(&target) = table.targets.get(slot as usize) {
            next!(vm, code, target as usize);
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}
