use std::alloc::Allocator;

use crate::ir::Scratch;
use purple_garden_ir::{self as ir};

/// Remove blocks the entry can't reach, dead SSA-producing instructions that have no observable
/// effect, and allocations only ever stored into, with their stores.
///
/// This runs to a fixed point because removing one dead producer can make
/// earlier producers dead too.
pub fn dce<F: Allocator, S: Allocator>(fun: &mut ir::Func<'_, F>, scratch: &mut Scratch<'_, S>) {
    unreachable_blocks(fun, scratch);
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

/// Tombstones every block the entry, the first live block, can't reach. Passes dropping an edge
/// leave its target behind, `switch_fold` the arm of a repeated key for instance. Nothing passes
/// such a block's params, the backends would still allocate and lower it: the x86 JIT takes a
/// block param's register class from the args passed to it and left a Double param of such a
/// block in a gpr, then panicked at the call taking it.
fn unreachable_blocks<F: Allocator, S: Allocator>(
    fun: &mut ir::Func<'_, F>,
    scratch: &Scratch<'_, S>,
) {
    let Some(entry) = fun.blocks.iter().position(|b| !b.tombstone) else {
        return;
    };
    let mut reached = Vec::new_in(scratch.alloc());
    reached.resize(fun.blocks.len(), false);
    let mut todo = Vec::new_in(scratch.alloc());
    reached[entry] = true;
    todo.push(entry);
    while let Some(block) = todo.pop() {
        if let Some(term) = &fun.blocks[block].term {
            fun.for_each_edge(term, |target, _| {
                let target = target.0 as usize;
                if !reached[target] {
                    reached[target] = true;
                    todo.push(target);
                }
            });
        }
    }
    for (block, reached) in fun.blocks.iter_mut().zip(reached) {
        if !reached && !block.tombstone {
            purple_garden_shared::trace!("[opt::ir::dce] b{} is unreachable", block.id.0);
            block.tombstone = true;
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

/// Whether `instr` only produces its value: no call, no trap. A division traps on a zero divisor
/// even when nothing reads its quotient, `x * 0` folding to `0` leaves exactly such a division
/// behind. An immediate divisor is only zero if the program divides by a literal zero.
fn removable(instr: &ir::Instr<'_>) -> bool {
    match instr {
        ir::Instr::Call { .. } => false,
        ir::Instr::Un {
            op: ir::UnOp::Assert(_),
            ..
        } => false,
        ir::Instr::Bin {
            op: ir::BinOp::IDiv | ir::BinOp::IMod,
            ..
        } => false,
        ir::Instr::BinImm {
            op: ir::BinOp::IDiv | ir::BinOp::IMod,
            imm,
            ..
        } => *imm != 0,
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

    /// `%v2 = %v0 op %v1` with an unused quotient: a division may trap and stays, an addition
    /// goes.
    #[test]
    fn keeps_divisions_with_an_unused_value() {
        use ir::BinOp;
        for (op, kept) in [
            (BinOp::IDiv, true),
            (BinOp::IMod, true),
            (BinOp::IAdd, false),
        ] {
            let int = |id| TypeId {
                id: Id(id),
                ty: Type::Int,
            };
            let mut fun = ir::Func::new("f", Id(1), vec![int(0), int(1)], Some(Type::Int));
            let params = fun.intern_params(vec![Id(0), Id(1)]);
            fun.blocks = vec![Block {
                tombstone: false,
                id: Id(0),
                params,
                instructions: vec![Instr::Bin {
                    op,
                    dst: int(2),
                    lhs: Id(0),
                    rhs: Id(1),
                    span: 0,
                }],
                term: Some(Terminator::Return {
                    value: Some(Id(0)),
                    span: 0,
                }),
            }];

            dce(&mut fun, &mut Scratch::default());
            assert_eq!(
                matches!(fun.blocks[0].instructions[0], Instr::Bin { .. }),
                kept,
                "{op:?}"
            );
        }
    }

    /// b0 returns, b1 jumps to b2 which returns: b1 and b2 have no path from b0.
    #[test]
    fn tombstones_unreachable_blocks() {
        let mut fun = ir::Func::new("f", Id(0), vec![], None);
        let params = fun.intern_params(vec![]);
        let block = |id, term| Block {
            tombstone: false,
            id: Id(id),
            params,
            instructions: vec![],
            term: Some(term),
        };
        let ret = Terminator::Return {
            value: None,
            span: 0,
        };
        fun.blocks = vec![
            block(0, ret.clone()),
            block(
                1,
                Terminator::Jump {
                    id: Id(2),
                    params,
                    span: 0,
                },
            ),
            block(2, ret),
        ];

        dce(&mut fun, &mut Scratch::default());
        let dead: Vec<bool> = fun.blocks.iter().map(|b| b.tombstone).collect();
        assert_eq!(dead, [false, true, true]);
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
