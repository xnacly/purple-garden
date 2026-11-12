use std::alloc::Allocator;

use purple_garden_ir as ir;

/// follow the jmp chain (if there is one) to figure out what params come out of a block unchanged
fn returned_param<F: Allocator>(fun: &ir::Func<'_, F>, id: ir::Id) -> Option<usize> {
    let block = &fun.blocks[id.0 as usize];
    if block.tombstone || !block.instructions.is_empty() {
        return None;
    }
    let returned = match &block.term {
        Some(ir::Terminator::Return { value: Some(v), .. }) => *v,
        Some(ir::Terminator::Jump { id, params, .. }) => {
            fun.params(*params)[returned_param(fun, *id)?]
        }
        _ => return None,
    };
    fun.params(block.params).iter().position(|p| *p == returned)
}

/// Rewrite `jmp b_join(v); b_join(p): ret p` as a direct `ret v` in the
/// predecessor, also through joins that only forward to another join, as a
/// nested if/match lowers to. Tombstones the joins left without predecessors.
/// Lets each arm of an if/match get its own Return, so the regalloc can
/// place the result in r0 instead of routing through a join-block param.
pub fn ret_inline<F: Allocator, S: Allocator>(
    fun: &mut ir::Func<'_, F>,
    scratch: &mut super::Scratch<'_, S>,
) {
    let alloc = scratch.alloc();
    let mut rewrites: Vec<(usize, ir::Id, u32), _> = Vec::new_in(alloc);
    let mut touched_targets = Vec::with_capacity_in(fun.blocks.len(), alloc);
    touched_targets.resize(fun.blocks.len(), false);

    for i in 0..fun.blocks.len() {
        if fun.blocks[i].tombstone {
            continue;
        }
        let Some(ir::Terminator::Jump {
            id: target_id,
            params: jump_params,
            span,
        }) = fun.blocks[i].term
        else {
            continue;
        };
        let Some(k) = returned_param(fun, target_id) else {
            continue;
        };

        rewrites.push((i, fun.params(jump_params)[k], span));
        touched_targets[target_id.0 as usize] = true;
    }

    for (i, ret_value, span) in rewrites {
        purple_garden_shared::trace!("[opt::ir::ret_inline] b{} inlines its jump-to-ret join", i);
        fun.blocks[i].term = Some(ir::Terminator::Return {
            value: Some(ret_value),
            span,
        });
    }

    let pred_counts = super::predecessor_counts(fun, alloc);
    for (target_id, touched) in touched_targets.into_iter().enumerate() {
        if touched && pred_counts[target_id] == 0 {
            purple_garden_shared::trace!(
                "[opt::ir::ret_inline] b{target_id} is now a tombstone (no predecessors)"
            );
            fun.blocks[target_id].tombstone = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ret_inline;
    use purple_garden_ir::{self as ir, Block, Id, Instr, Terminator, TypeId, ptype::Type};

    /// Single predecessor: `b0: jmp b1(%v0)` followed by `b1(%v1): ret %v1`
    /// collapses to `b0: ret %v0` and tombstones b1.
    #[test]
    fn inlines_single_predecessor_and_tombstones_join() {
        let mut fun = ir::Func::new(
            "f",
            Id(0),
            vec![TypeId {
                id: Id(0),
                ty: Type::Int,
            }],
            Some(Type::Int),
        );
        let b0_params = fun.intern_params(vec![Id(0)]);
        let b1_params = fun.intern_params(vec![Id(1)]);
        let jump_params = fun.intern_params(vec![Id(0)]);
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params: b0_params,
                instructions: vec![],
                term: Some(Terminator::Jump {
                    id: Id(1),
                    params: jump_params,
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params: b1_params,
                instructions: vec![],
                term: Some(Terminator::Return {
                    value: Some(Id(1)),
                    span: 0,
                }),
            },
        ];

        ret_inline(&mut fun, &mut super::super::Scratch::default());

        assert!(
            matches!(
                &fun.blocks[0].term,
                Some(Terminator::Return { value: Some(v), .. }) if v.0 == 0
            ),
            "predecessor must now return %v0 directly"
        );
        assert!(
            fun.blocks[1].tombstone,
            "join with no preds must be tombstoned"
        );
    }

    /// All-predecessors-inlined case: two arms both jump to a trivial
    /// return block. Both become Returns, and the join is tombstoned.
    #[test]
    fn inlines_all_arms_and_tombstones_join() {
        let mut fun = ir::Func::new(
            "f",
            Id(0),
            vec![TypeId {
                id: Id(0),
                ty: Type::Int,
            }],
            Some(Type::Int),
        );
        let b0_params = fun.intern_params(vec![Id(0)]);
        let b1_params = fun.intern_params(vec![Id(0)]);
        let b2_params = fun.intern_params(vec![Id(0)]);
        let b3_params = fun.intern_params(vec![Id(3)]);
        let br_yes = fun.intern_params(vec![Id(0)]);
        let br_no = fun.intern_params(vec![Id(0)]);
        let jmp_from_b1 = fun.intern_params(vec![Id(1)]);
        let jmp_from_b2 = fun.intern_params(vec![Id(2)]);
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params: b0_params,
                instructions: vec![],
                term: Some(Terminator::Branch {
                    cond: Id(0),
                    yes: (Id(1), br_yes),
                    no: (Id(2), br_no),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params: b1_params,
                instructions: vec![Instr::LoadConst {
                    dst: TypeId {
                        id: Id(1),
                        ty: Type::Int,
                    },
                    value: ir::constant::Const::Int(7),
                    span: 0,
                }],
                term: Some(Terminator::Jump {
                    id: Id(3),
                    params: jmp_from_b1,
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(2),
                params: b2_params,
                instructions: vec![Instr::LoadConst {
                    dst: TypeId {
                        id: Id(2),
                        ty: Type::Int,
                    },
                    value: ir::constant::Const::Int(8),
                    span: 0,
                }],
                term: Some(Terminator::Jump {
                    id: Id(3),
                    params: jmp_from_b2,
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(3),
                params: b3_params,
                instructions: vec![],
                term: Some(Terminator::Return {
                    value: Some(Id(3)),
                    span: 0,
                }),
            },
        ];

        ret_inline(&mut fun, &mut super::super::Scratch::default());

        assert!(
            matches!(&fun.blocks[1].term, Some(Terminator::Return { value: Some(v), .. }) if v.0 == 1)
        );
        assert!(
            matches!(&fun.blocks[2].term, Some(Terminator::Return { value: Some(v), .. }) if v.0 == 2)
        );
        assert!(
            fun.blocks[3].tombstone,
            "join is dead after both arms inline"
        );
    }

    /// A join block that returns a value that is none of its params
    /// (e.g. a literal it computed itself) cannot be inlined: we'd have
    /// to copy that computation into every predecessor.
    #[test]
    fn does_not_inline_when_return_does_not_alias_join_param() {
        let mut fun = ir::Func::new(
            "f",
            Id(0),
            vec![TypeId {
                id: Id(0),
                ty: Type::Int,
            }],
            Some(Type::Int),
        );
        let b0_params = fun.intern_params(vec![Id(0)]);
        let b1_params = fun.intern_params(vec![Id(1)]);
        let jump_params = fun.intern_params(vec![Id(0)]);
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params: b0_params,
                instructions: vec![],
                term: Some(Terminator::Jump {
                    id: Id(1),
                    params: jump_params,
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params: b1_params,
                instructions: vec![],
                // Returns %v9, not %v1 (the param). Cannot inline.
                term: Some(Terminator::Return {
                    value: Some(Id(9)),
                    span: 0,
                }),
            },
        ];

        ret_inline(&mut fun, &mut super::super::Scratch::default());

        assert!(
            matches!(
                &fun.blocks[0].term,
                Some(Terminator::Jump { id, .. }) if id.0 == 1
            ),
            "predecessor must stay as a Jump"
        );
        assert!(!fun.blocks[1].tombstone);
    }

    /// A non-trivial join (one with instructions) is not inlinable.
    #[test]
    fn does_not_inline_join_with_instructions() {
        let mut fun = ir::Func::new(
            "f",
            Id(0),
            vec![TypeId {
                id: Id(0),
                ty: Type::Int,
            }],
            Some(Type::Int),
        );
        let b0_params = fun.intern_params(vec![Id(0)]);
        let b1_params = fun.intern_params(vec![Id(1)]);
        let jump_params = fun.intern_params(vec![Id(0)]);
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params: b0_params,
                instructions: vec![],
                term: Some(Terminator::Jump {
                    id: Id(1),
                    params: jump_params,
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params: b1_params,
                instructions: vec![Instr::LoadConst {
                    dst: TypeId {
                        id: Id(2),
                        ty: Type::Int,
                    },
                    value: ir::constant::Const::Int(0),
                    span: 0,
                }],
                term: Some(Terminator::Return {
                    value: Some(Id(1)),
                    span: 0,
                }),
            },
        ];

        ret_inline(&mut fun, &mut super::super::Scratch::default());

        assert!(matches!(&fun.blocks[0].term, Some(Terminator::Jump { .. })));
        assert!(!fun.blocks[1].tombstone);
    }

    /// Nested match: the inner join b4 only forwards to the outer join b3, as
    /// lowered for `match { c { 7 } { match { ... { 8 } } } }`. Both arms must
    /// return directly and both joins become dead.
    #[test]
    fn inlines_through_forwarding_join() {
        let mut fun = ir::Func::new(
            "f",
            Id(0),
            vec![TypeId {
                id: Id(0),
                ty: Type::Int,
            }],
            Some(Type::Int),
        );
        let b0_params = fun.intern_params(vec![Id(0)]);
        let b1_params = fun.intern_params(vec![Id(1)]);
        let b2_params = fun.intern_params(vec![Id(3)]);
        let b3_params = fun.intern_params(vec![Id(5), Id(6)]);
        let b4_params = fun.intern_params(vec![Id(7), Id(8)]);
        let br_yes = fun.intern_params(vec![Id(0)]);
        let br_no = fun.intern_params(vec![Id(0)]);
        let jmp_from_b1 = fun.intern_params(vec![Id(2), Id(1)]);
        let jmp_from_b2 = fun.intern_params(vec![Id(4), Id(3)]);
        let jmp_from_b4 = fun.intern_params(vec![Id(7), Id(8)]);
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params: b0_params,
                instructions: vec![],
                term: Some(Terminator::Branch {
                    cond: Id(0),
                    yes: (Id(1), br_yes),
                    no: (Id(2), br_no),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params: b1_params,
                instructions: vec![Instr::LoadConst {
                    dst: TypeId {
                        id: Id(2),
                        ty: Type::Int,
                    },
                    value: ir::constant::Const::Int(7),
                    span: 0,
                }],
                term: Some(Terminator::Jump {
                    id: Id(3),
                    params: jmp_from_b1,
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(2),
                params: b2_params,
                instructions: vec![Instr::LoadConst {
                    dst: TypeId {
                        id: Id(4),
                        ty: Type::Int,
                    },
                    value: ir::constant::Const::Int(8),
                    span: 0,
                }],
                term: Some(Terminator::Jump {
                    id: Id(4),
                    params: jmp_from_b2,
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(3),
                params: b3_params,
                instructions: vec![],
                term: Some(Terminator::Return {
                    value: Some(Id(5)),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(4),
                params: b4_params,
                instructions: vec![],
                term: Some(Terminator::Jump {
                    id: Id(3),
                    params: jmp_from_b4,
                    span: 0,
                }),
            },
        ];

        ret_inline(&mut fun, &mut super::super::Scratch::default());

        assert!(
            matches!(&fun.blocks[1].term, Some(Terminator::Return { value: Some(v), .. }) if v.0 == 2)
        );
        assert!(
            matches!(&fun.blocks[2].term, Some(Terminator::Return { value: Some(v), .. }) if v.0 == 4),
            "arm jumping to the forwarding join must return directly"
        );
        assert!(fun.blocks[3].tombstone, "outer join is dead");
        assert!(fun.blocks[4].tombstone, "forwarding join is dead");
    }

    /// The returned param is tracked by position: b2 returns its second param,
    /// which b1 fills with its first, so b1 returns %v2 and b0 returns %v0.
    #[test]
    fn inlines_returned_param_at_any_position() {
        let mut fun = ir::Func::new(
            "f",
            Id(0),
            vec![
                TypeId {
                    id: Id(0),
                    ty: Type::Int,
                },
                TypeId {
                    id: Id(1),
                    ty: Type::Int,
                },
            ],
            Some(Type::Int),
        );
        let b0_params = fun.intern_params(vec![Id(0), Id(1)]);
        let b1_params = fun.intern_params(vec![Id(2), Id(3)]);
        let b2_params = fun.intern_params(vec![Id(4), Id(5)]);
        let jmp_from_b0 = fun.intern_params(vec![Id(0), Id(1)]);
        let jmp_from_b1 = fun.intern_params(vec![Id(3), Id(2)]);
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params: b0_params,
                instructions: vec![],
                term: Some(Terminator::Jump {
                    id: Id(1),
                    params: jmp_from_b0,
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params: b1_params,
                instructions: vec![],
                term: Some(Terminator::Jump {
                    id: Id(2),
                    params: jmp_from_b1,
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(2),
                params: b2_params,
                instructions: vec![],
                term: Some(Terminator::Return {
                    value: Some(Id(5)),
                    span: 0,
                }),
            },
        ];

        ret_inline(&mut fun, &mut super::super::Scratch::default());

        assert!(
            matches!(&fun.blocks[0].term, Some(Terminator::Return { value: Some(v), .. }) if v.0 == 0)
        );
        assert!(
            matches!(&fun.blocks[1].term, Some(Terminator::Return { value: Some(v), .. }) if v.0 == 2)
        );
        assert!(fun.blocks[1].tombstone);
        assert!(fun.blocks[2].tombstone);
    }
}
