use std::alloc::Allocator;

use crate::ir::Scratch;
use purple_garden_ir::{self as ir};

/// Remove dead SSA-producing instructions that have no observable effect.
///
/// This runs to a fixed point because removing one dead producer can make
/// earlier producers dead too.
pub fn dce<F: Allocator, S: Allocator>(fun: &mut ir::Func<'_, F>, scratch: &mut Scratch<'_, S>) {
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
        ir::Instr::Call { .. } | ir::Instr::Assert { .. } => false,
        ir::Instr::Sys { fun, .. } => fun.pure,
        ir::Instr::Noop => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::dce;
    use crate::ir::Scratch;
    use purple_garden_ir::{self as ir, Block, Id, Instr, Terminator, TypeId, ptype::Type};

    /// Nothing reads an assert's Void value, its trap still has to happen.
    #[test]
    fn keeps_asserts_with_an_unused_value() {
        let mut fun = ir::Func::new(
            "f",
            Id(1),
            vec![TypeId {
                id: Id(0),
                ty: Type::Bool,
            }],
            None,
        );
        let params = fun.intern_params(vec![Id(0)]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![Instr::Assert {
                dst: TypeId {
                    id: Id(1),
                    ty: Type::Void,
                },
                cond: Id(0),
                span: 0,
            }],
            term: Some(Terminator::Return {
                value: None,
                span: 0,
            }),
        }];

        dce(&mut fun, &mut Scratch::default());
        assert!(matches!(
            fun.blocks[0].instructions[0],
            Instr::Assert { .. }
        ));
    }
}
