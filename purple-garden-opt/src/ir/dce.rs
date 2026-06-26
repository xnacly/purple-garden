use crate::ir::Scratch;
use purple_garden_ir::{self as ir};

/// Remove dead SSA-producing instructions that have no observable effect.
///
/// This runs to a fixed point because removing one dead producer can make
/// earlier producers dead too.
pub fn dce(fun: &mut ir::Func<'_>, scratch: &mut Scratch<'_>) {
    loop {
        let mut changed = false;

        super::record_uses(fun, scratch);

        for block in &mut fun.blocks {
            if block.tombstone {
                continue;
            }

            for instr in &mut block.instructions {
                let Some(dst) = ir::Func::def_of(instr) else {
                    continue;
                };

                if scratch.use_count(dst) != 0 {
                    continue;
                }

                if removable(instr) {
                    purple_garden_shared::trace!(
                        "[opt::ir::dce] removed dead definition %v{}",
                        dst.0
                    );
                    *instr = ir::Instr::Noop;
                    changed = true;
                }
            }
        }

        if !changed {
            break;
        }
    }
}

fn removable(instr: &ir::Instr<'_>) -> bool {
    match instr {
        ir::Instr::Call { .. } => false,
        ir::Instr::Sys { fun, .. } => fun.pure,
        ir::Instr::Noop => false,
        _ => true,
    }
}
