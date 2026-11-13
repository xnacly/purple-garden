use std::alloc::Allocator;

use crate::ir::Scratch;
use purple_garden_ir::{self as ir};

/// Remove dead SSA-producing instructions that have no observable effect, and allocations only
/// ever stored into, with their stores.
///
/// This runs to a fixed point because removing one dead producer can make
/// earlier producers dead too.
pub fn dce<F: Allocator, S: Allocator>(fun: &mut ir::Func<'_, F>, scratch: &mut Scratch<'_, S>) {
    let mut stores = Vec::new_in(scratch.alloc());
    loop {
        let mut changed = false;

        super::record_uses(fun, scratch);
        changed |= dead_allocs(fun, scratch, &mut stores);

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

/// Removes every `Alloc` whose only uses are as the base of a `Store`, with those stores: values
/// are immutable, so nothing else can read what they wrote.
fn dead_allocs<F: Allocator, S: Allocator, A: Allocator>(
    fun: &mut ir::Func<'_, F>,
    scratch: &Scratch<'_, S>,
    stores: &mut Vec<Option<u32>, A>,
) -> bool {
    // stores[id] counts the stores into %id if an Alloc defines it
    stores.clear();
    let slot = |stores: &mut Vec<Option<u32>, A>, id: ir::Id| {
        let i = id.0 as usize;
        if stores.len() <= i {
            stores.resize(i + 1, None);
        }
        i
    };
    for instr in fun
        .blocks
        .iter()
        .filter(|block| !block.tombstone)
        .flat_map(|block| &block.instructions)
    {
        match instr {
            ir::Instr::Alloc { dst, .. } => {
                let i = slot(stores, dst.id);
                stores[i].get_or_insert(0);
            }
            ir::Instr::Store { base, .. } => {
                let i = slot(stores, *base);
                if let Some(count) = &mut stores[i] {
                    *count += 1;
                }
            }
            _ => {}
        }
    }
    // a store counted short only keeps its Alloc, its use count stays above the stores
    let dead = |id: ir::Id| {
        stores
            .get(id.0 as usize)
            .copied()
            .flatten()
            .is_some_and(|stored| scratch.use_count(id) == stored)
    };

    let mut changed = false;
    for block in fun.blocks.iter_mut().filter(|block| !block.tombstone) {
        for instr in &mut block.instructions {
            let id = match instr {
                ir::Instr::Alloc { dst, .. } => dst.id,
                ir::Instr::Store { base, .. } => *base,
                _ => continue,
            };
            if dead(id) {
                purple_garden_shared::trace!(
                    "[opt::ir::dce] removed {instr:?}, %v{} is never read",
                    id.0
                );
                *instr = ir::Instr::Noop;
                changed = true;
            }
        }
    }
    changed
}

fn removable(instr: &ir::Instr<'_>) -> bool {
    match instr {
        ir::Instr::Call { .. } => false,
        ir::Instr::Un {
            op: ir::UnOp::Assert(_),
            ..
        } => false,
        ir::Instr::Sys { fun, .. } => fun.pure,
        ir::Instr::Noop => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::dce;
    use crate::ir::Scratch;
    use purple_garden_ir::{self as ir, Block, Id, Instr, Terminator, TypeId, UnOp, ptype::Type};
    use std::alloc::Layout;

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
            instructions: vec![Instr::Un {
                op: UnOp::Assert(ir::Check::Assertion),
                dst: TypeId {
                    id: Id(1),
                    ty: Type::Void,
                },
                src: Id(0),
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
            Instr::Un {
                op: UnOp::Assert(_),
                ..
            }
        ));
    }

    /// `%v0 = Alloc; Store %v0+0, %v1` then `tail`, returning `%v2`. `%v1` is the param.
    fn alloc_then(tail: Vec<Instr<'static>>) -> ir::Func<'static> {
        let ty = |id, ty| TypeId { id: Id(id), ty };
        let mut fun = ir::Func::new("f", Id(0), vec![ty(1, Type::Int)], Some(Type::Int));
        let params = fun.intern_params([Id(1)]);
        let mut instructions = vec![
            Instr::Alloc {
                dst: ty(0, Type::Array(&Type::Int)),
                layout: Layout::from_size_align(8, 8).unwrap(),
                span: 0,
            },
            Instr::Store {
                src: Id(1),
                base: Id(0),
                offset: 0,
                span: 0,
            },
        ];
        instructions.extend(tail);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions,
            term: Some(Terminator::Return {
                value: Some(Id(2)),
                span: 0,
            }),
        }];
        dce(&mut fun, &mut Scratch::default());
        fun
    }

    fn removed(fun: &ir::Func<'_>) -> [bool; 2] {
        [0, 1].map(|i| matches!(fun.blocks[0].instructions[i], Instr::Noop))
    }

    #[test]
    fn removes_an_alloc_only_stored_into() {
        let fun = alloc_then(vec![Instr::LoadConst {
            dst: TypeId {
                id: Id(2),
                ty: Type::Int,
            },
            value: ir::constant::Const::Int(7),
            span: 0,
        }]);
        assert_eq!(removed(&fun), [true, true]);
    }

    #[test]
    fn keeps_an_alloc_read_back() {
        let fun = alloc_then(vec![Instr::Load {
            dst: TypeId {
                id: Id(2),
                ty: Type::Int,
            },
            base: Id(0),
            offset: 0,
            span: 0,
        }]);
        assert_eq!(removed(&fun), [false, false]);
    }

    /// Stored into another allocation, which is returned: the first one is reachable through it.
    #[test]
    fn keeps_an_alloc_stored_elsewhere() {
        let fun = alloc_then(vec![
            Instr::Alloc {
                dst: TypeId {
                    id: Id(2),
                    ty: Type::Array(&Type::Array(&Type::Int)),
                },
                layout: Layout::from_size_align(8, 8).unwrap(),
                span: 0,
            },
            Instr::Store {
                src: Id(0),
                base: Id(2),
                offset: 0,
                span: 0,
            },
        ]);
        assert_eq!(removed(&fun), [false, false]);
    }
}
