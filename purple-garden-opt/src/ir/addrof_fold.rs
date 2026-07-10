use purple_garden_ir::{self as ir, Id};

/// Block-local rewrites for `AddrOf` producers and consumers.
pub fn addrof_fold(fun: &mut ir::Func<'_>, scratch: &mut super::Scratch<'_>) {
    for block_idx in 0..fun.blocks.len() {
        if fun.blocks[block_idx].tombstone {
            continue;
        }

        scratch.reset();
        let mut defs = Vec::new();

        let block = &fun.blocks[block_idx];
        for instr in &block.instructions {
            if let Some(id) = ir::Func::def_of(instr) {
                scratch.ensure(id);
            }
            ir::Func::for_each_use_of_instr(instr, |id| scratch.bump(id));
            collect_addrof_def(instr, &mut defs);
        }
        if let Some(term) = &block.term {
            fun.for_each_use_of_term(term, |id| scratch.bump(id));
        }

        fold_block(&mut fun.blocks[block_idx].instructions, scratch, &defs);
    }
}

fn collect_addrof_def(instr: &ir::Instr<'_>, defs: &mut Vec<Option<(Id, u32)>>) {
    let ir::Instr::AddrOf {
        dst, base, offset, ..
    } = instr
    else {
        return;
    };

    let len = dst.id.0 as usize + 1;
    if defs.len() < len {
        defs.resize(len, None);
    }
    defs[dst.id.0 as usize] = Some((*base, *offset));
}

fn fold_block(
    instructions: &mut [ir::Instr<'_>],
    scratch: &super::Scratch<'_>,
    defs: &[Option<(Id, u32)>],
) {
    for instr in instructions {
        let Some((base, offset)) = base_offset_mut(instr) else {
            continue;
        };

        let Some((new_base, new_offset)) = resolve_addr(*base, *offset, scratch, defs) else {
            continue;
        };

        purple_garden_shared::trace!(
            "[opt::ir::addrof_fold] folded %v{}+{} into %v{}+{}",
            base.0,
            *offset,
            new_base.0,
            new_offset
        );

        *base = new_base;
        *offset = new_offset;
    }
}

fn base_offset_mut<'instr>(
    instr: &'instr mut ir::Instr<'_>,
) -> Option<(&'instr mut Id, &'instr mut u32)> {
    match instr {
        ir::Instr::Load { base, offset, .. }
        | ir::Instr::Store { base, offset, .. }
        | ir::Instr::AddrOf { base, offset, .. } => Some((base, offset)),
        _ => None,
    }
}

fn resolve_addr(
    mut base: Id,
    mut offset: u32,
    scratch: &super::Scratch<'_>,
    defs: &[Option<(Id, u32)>],
) -> Option<(Id, u32)> {
    let mut folded = false;

    loop {
        if scratch.use_count(base) != 1 {
            return folded.then_some((base, offset));
        }

        let Some((new_base, base_offset)) = defs.get(base.0 as usize).copied().flatten() else {
            return folded.then_some((base, offset));
        };

        offset = base_offset.checked_add(offset)?;
        base = new_base;
        folded = true;
    }
}

#[cfg(test)]
mod tests {
    use super::addrof_fold;
    use purple_garden_ir::{Block, Func, Id, Instr, Terminator, TypeId, ptype::Type};

    fn type_id(id: u32, ty: Type<'static>) -> TypeId<'static> {
        TypeId { id: Id(id), ty }
    }

    fn func_with_instructions(instructions: Vec<Instr<'static>>) -> Func<'static> {
        let mut fun = Func::new("f", Id(0), vec![Id(0)], Some(Type::Int));
        let params = fun.intern_params(vec![Id(0)]);
        fun.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions,
            term: Some(Terminator::Return {
                value: None,
                span: 0,
            }),
        });
        fun
    }

    fn run(fun: &mut Func<'static>) {
        let mut scratch = super::super::Scratch::default();
        addrof_fold(fun, &mut scratch);
    }

    #[test]
    fn skeleton_preserves_instructions() {
        let mut fun = Func::new("f", Id(0), Vec::new(), Some(Type::Int));
        let params = fun.intern_params(Vec::new());
        fun.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![Instr::Noop],
            term: Some(Terminator::Return {
                value: None,
                span: 0,
            }),
        });

        run(&mut fun);

        assert!(matches!(
            fun.blocks[0].instructions.as_slice(),
            [Instr::Noop]
        ));
    }

    #[test]
    fn folds_load_through_single_use_addrof() {
        let mut fun = func_with_instructions(vec![
            Instr::AddrOf {
                dst: type_id(1, Type::record(Vec::new())),
                base: Id(0),
                offset: 8,
                span: 0,
            },
            Instr::Load {
                dst: type_id(2, Type::Int),
                base: Id(1),
                offset: 4,
                span: 0,
            },
        ]);

        run(&mut fun);

        assert!(matches!(
            fun.blocks[0].instructions[1],
            Instr::Load {
                base: Id(0),
                offset: 12,
                ..
            }
        ));
    }

    #[test]
    fn folds_store_through_single_use_addrof() {
        let mut fun = func_with_instructions(vec![
            Instr::AddrOf {
                dst: type_id(1, Type::record(Vec::new())),
                base: Id(0),
                offset: 8,
                span: 0,
            },
            Instr::Store {
                src: Id(2),
                base: Id(1),
                offset: 4,
                span: 0,
            },
        ]);

        run(&mut fun);

        assert!(matches!(
            fun.blocks[0].instructions[1],
            Instr::Store {
                base: Id(0),
                offset: 12,
                ..
            }
        ));
    }

    #[test]
    fn folds_nested_single_use_addrof_chain() {
        let mut fun = func_with_instructions(vec![
            Instr::AddrOf {
                dst: type_id(1, Type::record(Vec::new())),
                base: Id(0),
                offset: 8,
                span: 0,
            },
            Instr::AddrOf {
                dst: type_id(2, Type::record(Vec::new())),
                base: Id(1),
                offset: 4,
                span: 0,
            },
            Instr::Load {
                dst: type_id(3, Type::Int),
                base: Id(2),
                offset: 2,
                span: 0,
            },
        ]);

        run(&mut fun);

        assert!(matches!(
            fun.blocks[0].instructions[1],
            Instr::AddrOf {
                base: Id(0),
                offset: 12,
                ..
            }
        ));
        assert!(matches!(
            fun.blocks[0].instructions[2],
            Instr::Load {
                base: Id(0),
                offset: 14,
                ..
            }
        ));
    }

    #[test]
    fn leaves_multi_use_addrof_base_alone() {
        let mut fun = func_with_instructions(vec![
            Instr::AddrOf {
                dst: type_id(1, Type::record(Vec::new())),
                base: Id(0),
                offset: 8,
                span: 0,
            },
            Instr::Load {
                dst: type_id(2, Type::Int),
                base: Id(1),
                offset: 0,
                span: 0,
            },
            Instr::Load {
                dst: type_id(3, Type::Int),
                base: Id(1),
                offset: 4,
                span: 0,
            },
        ]);

        run(&mut fun);

        assert!(matches!(
            fun.blocks[0].instructions[1],
            Instr::Load { base: Id(1), .. }
        ));
        assert!(matches!(
            fun.blocks[0].instructions[2],
            Instr::Load { base: Id(1), .. }
        ));
    }

    #[test]
    fn leaves_addrof_used_by_terminator_alone() {
        let mut fun = func_with_instructions(vec![
            Instr::AddrOf {
                dst: type_id(1, Type::record(Vec::new())),
                base: Id(0),
                offset: 8,
                span: 0,
            },
            Instr::Load {
                dst: type_id(2, Type::Int),
                base: Id(1),
                offset: 4,
                span: 0,
            },
        ]);
        fun.blocks[0].term = Some(Terminator::Return {
            value: Some(Id(1)),
            span: 0,
        });

        run(&mut fun);

        assert!(matches!(
            fun.blocks[0].instructions[1],
            Instr::Load {
                base: Id(1),
                offset: 4,
                ..
            }
        ));
    }

    #[test]
    fn leaves_overflowing_offset_alone() {
        let mut fun = func_with_instructions(vec![
            Instr::AddrOf {
                dst: type_id(1, Type::record(Vec::new())),
                base: Id(0),
                offset: u32::MAX,
                span: 0,
            },
            Instr::Load {
                dst: type_id(2, Type::Int),
                base: Id(1),
                offset: 1,
                span: 0,
            },
        ]);

        run(&mut fun);

        assert!(matches!(
            fun.blocks[0].instructions[1],
            Instr::Load {
                base: Id(1),
                offset: 1,
                ..
            }
        ));
    }

    #[test]
    fn does_not_fold_across_blocks() {
        let mut fun = Func::new("f", Id(0), vec![Id(0)], Some(Type::Int));
        let entry_params = fun.intern_params(vec![Id(0)]);
        let empty_params = fun.intern_params(Vec::new());
        fun.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params: entry_params,
            instructions: vec![Instr::AddrOf {
                dst: type_id(1, Type::record(Vec::new())),
                base: Id(0),
                offset: 8,
                span: 0,
            }],
            term: Some(Terminator::Jump {
                id: Id(1),
                params: empty_params,
                span: 0,
            }),
        });
        fun.blocks.push(Block {
            tombstone: false,
            id: Id(1),
            params: empty_params,
            instructions: vec![Instr::Load {
                dst: type_id(2, Type::Int),
                base: Id(1),
                offset: 4,
                span: 0,
            }],
            term: Some(Terminator::Return {
                value: None,
                span: 0,
            }),
        });

        run(&mut fun);

        assert!(matches!(
            fun.blocks[1].instructions[0],
            Instr::Load {
                base: Id(1),
                offset: 4,
                ..
            }
        ));
    }
}
