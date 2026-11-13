use std::alloc::Allocator;

use purple_garden_ir::{self as ir, Id};

/// Forwards a stored value to the loads of its address in the same block:
///
/// ```text
/// Store %v0+0, %v1
/// %v4:Int = Load %v0+0      // removed, its uses read %v1
/// ```
///
/// Values are immutable, only the initialisation of a fresh allocation stores, so nothing writes
/// an address between its store and a load of it.
pub fn load_store_fold<F: Allocator, S: Allocator>(
    fun: &mut ir::Func<'_, F>,
    scratch: &mut super::Scratch<'_, S>,
) {
    scratch.reset();
    let mut stored = Vec::new_in(scratch.alloc());

    for block in &mut fun.blocks {
        if block.tombstone {
            continue;
        }
        stored.clear();
        for instr in &mut block.instructions {
            match instr {
                ir::Instr::Store {
                    src, base, offset, ..
                } => stored.push((*base, *offset, *src)),
                ir::Instr::Load {
                    dst, base, offset, ..
                } => {
                    let Some(src) = stored_at(&stored, *base, *offset) else {
                        continue;
                    };
                    purple_garden_shared::trace!(
                        "[opt::ir::load_store_fold] %v{} is the stored %v{}",
                        dst.id.0,
                        src.0
                    );
                    scratch.alias(dst.id, src);
                    *instr = ir::Instr::Noop;
                }
                _ => {}
            }
        }
    }

    if !scratch.remap().is_empty() {
        fun.replace_uses(scratch.remap());
    }
}

fn stored_at(stored: &[(Id, u32, Id)], base: Id, offset: u32) -> Option<Id> {
    stored
        .iter()
        .find(|&&(b, o, _)| b == base && o == offset)
        .map(|&(_, _, src)| src)
}

#[cfg(test)]
mod tests {
    use super::load_store_fold;
    use crate::ir::Scratch;
    use purple_garden_ir::{Block, Func, Id, Instr, Terminator, TypeId, ptype::Type};
    use std::alloc::Layout;

    /// `%v0 = Alloc; Store %v0+0, %v1` then a load of `%v0+offset` returned.
    fn store_then_load(offset: u32) -> Func<'static> {
        let int = |id| TypeId {
            id: Id(id),
            ty: Type::Int,
        };
        let mut fun = Func::new("f", Id(0), vec![int(1)], Some(Type::Int));
        let params = fun.intern_params([Id(1)]);
        fun.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::Alloc {
                    dst: TypeId {
                        id: Id(0),
                        ty: Type::Array(&Type::Int),
                    },
                    layout: Layout::from_size_align(16, 8).unwrap(),
                    span: 0,
                },
                Instr::Store {
                    src: Id(1),
                    base: Id(0),
                    offset: 0,
                    span: 0,
                },
                Instr::Load {
                    dst: int(2),
                    base: Id(0),
                    offset,
                    span: 0,
                },
            ],
            term: Some(Terminator::Return {
                value: Some(Id(2)),
                span: 0,
            }),
        });
        load_store_fold(&mut fun, &mut Scratch::default());
        fun
    }

    #[test]
    fn forwards_the_stored_value() {
        let fun = store_then_load(0);
        assert!(matches!(fun.blocks[0].instructions[2], Instr::Noop));
        assert!(matches!(
            fun.blocks[0].term,
            Some(Terminator::Return {
                value: Some(Id(1)),
                ..
            })
        ));
    }

    #[test]
    fn keeps_a_load_of_another_address() {
        let fun = store_then_load(8);
        assert!(matches!(fun.blocks[0].instructions[2], Instr::Load { .. }));
    }
}
