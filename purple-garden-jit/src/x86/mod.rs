//! x86-64 JIT lowering.
//!
//! Baseline: a dispatch remover, not an optimizer. The native ABI passes
//! `*mut Vm` in `rdi`, and since `Vm::r` is the first field, `rdi` doubles as
//! the base of the VM register file; generated code just moves between those
//! 8-byte slots and returns.

use purple_garden_ir as ir;

/// Bail out of [`compile_func`] (returning `None`) and, under the `trace`
/// feature, log why. The reason is only formatted inside `trace!`, so it costs
/// nothing when the feature is off; the whole diagnostic is trace-guarded.
macro_rules! skip {
    ($func:expr, $($reason:tt)*) => {{
        purple_garden_shared::trace!(
            "[jit::x86] skipped {}: {}",
            $func.name,
            format_args!($($reason)*)
        );
        return None;
    }};
}

/// `regs` is the backend's per-SSA register assignment, indexed by [`ir::Id`].
/// Lowers a function into `code`, or returns `None` (leaving `code` untouched
/// from the caller's perspective; it clears first) if it uses anything the
/// baseline can't handle yet: spilled values, multiple blocks, non-`Noop`
/// instructions, non-`Return` terminators.
pub fn compile_func(func: &ir::Func<'_>, regs: &[ir::Location], code: &mut Vec<u8>) -> Option<()> {
    let mut blocks = func.blocks.iter().filter(|block| !block.tombstone);
    let Some(block) = blocks.next() else {
        skip!(func, "empty function");
    };
    if blocks.next().is_some() {
        skip!(func, "multiple blocks");
    }
    for instr in &block.instructions {
        if !matches!(instr, ir::Instr::Noop) {
            skip!(func, "unsupported instruction {instr:?}");
        }
    }

    let term = block.term.as_ref();
    match term {
        None => {}
        Some(ir::Terminator::Return { value, .. }) => {
            if let Some(value) = value {
                let Some(src) = reg(regs, *value) else {
                    skip!(func, "return uses undefined SSA value %v{}", value.0);
                };
                // r0 is the return slot; only move when the value lives elsewhere.
                if src != 0 {
                    emit_mov(code, 0, src);
                }
            }
        }
        _ => skip!(func, "unsupported terminator {term:?}"),
    }
    code.push(0xc3); // ret

    purple_garden_shared::trace!("[jit::x86] compiled {} ({} bytes)", func.name, code.len());
    Some(())
}

/// SSA value -> its assigned VM register, or `None` if spilled/unassigned.
fn reg(regs: &[ir::Location], id: ir::Id) -> Option<u8> {
    match regs.get(id.0 as usize) {
        Some(ir::Location::Reg(r)) => Some(*r),
        _ => None,
    }
}

/// `mov r{dst} <- r{src}` through the VM register file (base in `rdi`):
/// `mov rax, [rdi + src*8]` then `mov [rdi + dst*8], rax`.
fn emit_mov(code: &mut Vec<u8>, dst: u8, src: u8) {
    code.extend_from_slice(&[0x48, 0x8b, 0x87]);
    code.extend_from_slice(&(i32::from(src) * 8).to_le_bytes());
    code.extend_from_slice(&[0x48, 0x89, 0x87]);
    code.extend_from_slice(&(i32::from(dst) * 8).to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::emit_mov;

    #[test]
    fn mov_template_bytes() {
        // mov rax, [rdi + 3*8]; mov [rdi + 4*8], rax
        let mut code = Vec::new();
        emit_mov(&mut code, 4, 3);
        assert_eq!(
            code,
            &[0x48, 0x8b, 0x87, 0x18, 0, 0, 0, 0x48, 0x89, 0x87, 0x20, 0, 0, 0]
        );
    }
}
