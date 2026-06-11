use purple_garden_ir as ir;

/// Rewrite `jmp b_join(v); b_join(p): ret p` as a direct `ret v` in the
/// predecessor. Tombstones the join when every predecessor is inlined.
/// Lets each arm of an if/match get its own Return, so the regalloc can
/// place the result in r0 instead of routing through a join-block param.
pub fn ret_inline(fun: &mut ir::Func) {
    let mut rewrites: Vec<(usize, ir::Id, u32)> = Vec::new();
    let mut pred_counts = predecessor_counts(fun);
    let mut touched_targets = vec![false; fun.blocks.len()];

    for i in 0..fun.blocks.len() {
        if fun.blocks[i].tombstone {
            continue;
        }
        let Some(ir::Terminator::Jump {
            id: target_id,
            params: jump_params,
            span,
        }) = fun.blocks[i].term.clone()
        else {
            continue;
        };

        let target = &fun.blocks[target_id.0 as usize];
        if target.tombstone || !target.instructions.is_empty() {
            continue;
        }
        let Some(ir::Terminator::Return {
            value: Some(ret_id),
            ..
        }) = &target.term
        else {
            continue;
        };

        // Only match when the join returns its first param verbatim, so
        // substituting the jump's first arg is sound.
        let target_params = fun.params(target.params);
        if target_params.is_empty() || target_params[0].0 != ret_id.0 {
            continue;
        }

        let jump_params_resolved = fun.params(jump_params);
        if jump_params_resolved.is_empty() {
            continue;
        }
        let ret_value = jump_params_resolved[0];

        rewrites.push((i, ret_value, span));
        let target_idx = target_id.0 as usize;
        pred_counts[target_idx] = pred_counts[target_idx].saturating_sub(1);
        touched_targets[target_idx] = true;
    }

    for (i, ret_value, span) in rewrites {
        purple_garden_shared::trace!("[opt::ir::ret_inline] b{} inlines its jump-to-ret join", i);
        fun.blocks[i].term = Some(ir::Terminator::Return {
            value: Some(ret_value),
            span,
        });
    }

    for (target_id, touched) in touched_targets.into_iter().enumerate() {
        if touched && pred_counts[target_id] == 0 {
            purple_garden_shared::trace!(
                "[opt::ir::ret_inline] b{target_id} is now a tombstone (no predecessors)"
            );
            fun.blocks[target_id].tombstone = true;
        }
    }
}

fn predecessor_counts(fun: &ir::Func) -> Vec<u32> {
    let mut counts = vec![0; fun.blocks.len()];

    for block in &fun.blocks {
        if block.tombstone {
            continue;
        }

        match &block.term {
            Some(ir::Terminator::Jump { id, .. }) => counts[id.0 as usize] += 1,
            Some(ir::Terminator::Branch { yes, no, .. })
            | Some(ir::Terminator::BranchCmpImm { yes, no, .. }) => {
                counts[yes.0.0 as usize] += 1;
                counts[no.0.0 as usize] += 1;
            }
            _ => {}
        }
    }

    counts
}

#[cfg(test)]
mod tests {
    use super::ret_inline;
    use purple_garden_ir::{self as ir, Block, Id, Instr, Terminator, TypeId, ptype::Type};

    /// Single predecessor: `b0: jmp b1(%v0)` followed by `b1(%v1): ret %v1`
    /// collapses to `b0: ret %v0` and tombstones b1.
    #[test]
    fn inlines_single_predecessor_and_tombstones_join() {
        let mut fun = ir::Func::new("f", Id(0), vec![Id(0)], Some(Type::Int));
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

        ret_inline(&mut fun);

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
        let mut fun = ir::Func::new("f", Id(0), vec![Id(0)], Some(Type::Int));
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

        ret_inline(&mut fun);

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

    /// A join block that returns a value NOT equal to its first param
    /// (e.g. a literal it computed itself) cannot be inlined: we'd have
    /// to copy that computation into every predecessor.
    #[test]
    fn does_not_inline_when_return_does_not_alias_join_param() {
        let mut fun = ir::Func::new("f", Id(0), vec![Id(0)], Some(Type::Int));
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

        ret_inline(&mut fun);

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
        let mut fun = ir::Func::new("f", Id(0), vec![Id(0)], Some(Type::Int));
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

        ret_inline(&mut fun);

        assert!(matches!(&fun.blocks[0].term, Some(Terminator::Jump { .. })));
        assert!(!fun.blocks[1].tombstone);
    }
}
