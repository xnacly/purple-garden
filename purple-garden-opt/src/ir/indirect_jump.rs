use std::alloc::Allocator;

use purple_garden_ir as ir;

/// merges a br to block A indirected by a single jmp in block B into a direct br to A:
///
/// ```text
///     b1:
///             %v4:Bool = True
///             br %v4, b2, b3
///     b2:
///             jmp b4(%v1) // <- redundant
///     b3:
///     b4:
/// ```
///
/// Should be:
/// ```text
///     b1:
///             %v4:Bool = True
///             br %v4, b4, b3
///     b3:
///     b4:
/// ```
///
/// b2's params are only bound by the edge into it, so the jmp's args are rewritten in terms of
/// the br's args; b2 is tombstoned once no edge leads into it anymore.
pub fn indirect_jump<F: Allocator>(fun: &mut ir::Func<'_, F>) {
    let mut preds = vec![0u32; fun.blocks.len()];
    for block in fun.blocks.iter().filter(|block| !block.tombstone) {
        for_each_target(fun, &block.term, |target| preds[target as usize] += 1);
    }

    for i in 0..fun.blocks.len() {
        if fun.blocks[i].tombstone {
            continue;
        }
        let Some((yes, yes_params, no, no_params)) = branch_edges(&fun.blocks[i].term) else {
            continue;
        };
        let yes_edge = forward(fun, &mut preds, yes, yes_params);
        let no_edge = forward(fun, &mut preds, no, no_params);
        rewrite_branch_edges(&mut fun.blocks[i].term, yes_edge, no_edge);
    }
}

/// The edge into `target(args)`, taken straight to the jmp's destination if `target` is empty
/// but for that jmp.
fn forward<F: Allocator>(
    fun: &mut ir::Func<'_, F>,
    preds: &mut [u32],
    target: u32,
    args: ir::ParamsId,
) -> (ir::Id, ir::ParamsId) {
    let block = &fun.blocks[target as usize];
    let (true, Some(ir::Terminator::Jump { id, params, .. })) =
        (block.instructions.is_empty(), &block.term)
    else {
        return (ir::Id(target), args);
    };
    let (dest, jump_args, block_params) = (*id, *params, block.params);
    if dest.0 == target {
        return (ir::Id(target), args);
    }

    let block_params = fun.params(block_params);
    let incoming = fun.params(args);
    let forwarded: Vec<ir::Id> = fun
        .params(jump_args)
        .iter()
        .map(|arg| {
            block_params
                .iter()
                .position(|param| param == arg)
                .map_or(*arg, |i| incoming[i])
        })
        .collect();
    let forwarded = fun.intern_params(forwarded);

    preds[dest.0 as usize] += 1;
    preds[target as usize] -= 1;
    if preds[target as usize] == 0 {
        purple_garden_shared::trace!("[opt::ir::indirect_jump] b{target} is now a tombstone");
        fun.blocks[target as usize].tombstone = true;
        preds[dest.0 as usize] -= 1;
    }
    (dest, forwarded)
}

fn for_each_target<F: Allocator>(
    fun: &ir::Func<'_, F>,
    term: &Option<ir::Terminator>,
    mut f: impl FnMut(u32),
) {
    match term {
        Some(ir::Terminator::Jump { id, .. }) => f(id.0),
        Some(
            ir::Terminator::Branch { yes, no, .. }
            | ir::Terminator::BranchCmpImm { yes, no, .. }
            | ir::Terminator::BranchCmp { yes, no, .. },
        ) => {
            f(yes.0.0);
            f(no.0.0);
        }
        Some(ir::Terminator::Switch { cases, default, .. }) => {
            for case in fun.cases(*cases) {
                f(case.target.0.0);
            }
            f(default.0.0);
        }
        Some(ir::Terminator::Return { .. } | ir::Terminator::Tail { .. }) | None => {}
    }
}

fn branch_edges(term: &Option<ir::Terminator>) -> Option<(u32, ir::ParamsId, u32, ir::ParamsId)> {
    match term {
        Some(ir::Terminator::Branch {
            yes: (ir::Id(yes), yes_params),
            no: (ir::Id(no), no_params),
            ..
        })
        | Some(ir::Terminator::BranchCmpImm {
            yes: (ir::Id(yes), yes_params),
            no: (ir::Id(no), no_params),
            ..
        })
        | Some(ir::Terminator::BranchCmp {
            yes: (ir::Id(yes), yes_params),
            no: (ir::Id(no), no_params),
            ..
        }) => Some((*yes, *yes_params, *no, *no_params)),
        _ => None,
    }
}

fn rewrite_branch_edges(
    term: &mut Option<ir::Terminator>,
    yes_edge: (ir::Id, ir::ParamsId),
    no_edge: (ir::Id, ir::ParamsId),
) {
    match term {
        Some(ir::Terminator::Branch { yes, no, .. })
        | Some(ir::Terminator::BranchCmpImm { yes, no, .. })
        | Some(ir::Terminator::BranchCmp { yes, no, .. }) => {
            *yes = yes_edge;
            *no = no_edge;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::indirect_jump;
    use purple_garden_ir::{self as ir, Block, Id, Terminator, TypeId, ptype::Type};

    /// Build a function where b0 branches to b1 (which jumps to b3) and
    /// b2 (which does real work). After `indirect_jump`, b0 should branch
    /// directly to b3 on the yes edge and b1 should be tombstoned.
    #[test]
    fn collapses_trivial_jump_through_yes_edge() {
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
        let b1_params = fun.intern_params(vec![]);
        let b2_params = fun.intern_params(vec![]);
        let b3_params = fun.intern_params(vec![]);
        let yes_br_params = fun.intern_params(vec![]);
        let no_br_params = fun.intern_params(vec![]);
        let jump_params = fun.intern_params(vec![]);
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params: b0_params,
                instructions: vec![],
                term: Some(Terminator::Branch {
                    cond: Id(0),
                    yes: (Id(1), yes_br_params),
                    no: (Id(2), no_br_params),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params: b1_params,
                instructions: vec![],
                term: Some(Terminator::Jump {
                    id: Id(3),
                    params: jump_params,
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(2),
                params: b2_params,
                instructions: vec![],
                term: Some(Terminator::Return {
                    value: Some(Id(0)),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(3),
                params: b3_params,
                instructions: vec![],
                term: Some(Terminator::Return {
                    value: Some(Id(0)),
                    span: 0,
                }),
            },
        ];

        indirect_jump(&mut fun);

        assert!(
            matches!(
                &fun.blocks[0].term,
                Some(Terminator::Branch { yes: (Id(y), _), no: (Id(n), _), .. })
                    if *y == 3 && *n == 2
            ),
            "yes edge should retarget to b3, no edge unchanged"
        );
        assert!(fun.blocks[1].tombstone, "trivial b1 must be tombstoned");
        assert!(!fun.blocks[3].tombstone, "real target b3 must survive");
    }

    /// A non-trivial intermediate (jump block with instructions) is not
    /// collapsed; its edge must point to the original block.
    #[test]
    fn leaves_non_trivial_intermediate_alone() {
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
        let b1_params = fun.intern_params(vec![]);
        let b2_params = fun.intern_params(vec![]);
        let b3_params = fun.intern_params(vec![]);
        let yes_br_params = fun.intern_params(vec![]);
        let no_br_params = fun.intern_params(vec![]);
        let jump_params = fun.intern_params(vec![]);
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params: b0_params,
                instructions: vec![],
                term: Some(Terminator::Branch {
                    cond: Id(0),
                    yes: (Id(1), yes_br_params),
                    no: (Id(2), no_br_params),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params: b1_params,
                instructions: vec![ir::Instr::LoadConst {
                    dst: ir::TypeId {
                        id: Id(7),
                        ty: Type::Int,
                    },
                    value: ir::constant::Const::Int(0),
                    span: 0,
                }],
                term: Some(Terminator::Jump {
                    id: Id(3),
                    params: jump_params,
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(2),
                params: b2_params,
                instructions: vec![],
                term: Some(Terminator::Return {
                    value: Some(Id(0)),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(3),
                params: b3_params,
                instructions: vec![],
                term: Some(Terminator::Return {
                    value: Some(Id(0)),
                    span: 0,
                }),
            },
        ];

        indirect_jump(&mut fun);

        assert!(
            matches!(
                &fun.blocks[0].term,
                Some(Terminator::Branch { yes: (Id(y), _), .. }) if *y == 1
            ),
            "yes edge must stay on b1 (it has instructions)"
        );
        assert!(!fun.blocks[1].tombstone);
    }

    /// b0 branches into b1(%v0) and b2; b1(%v1) only jumps to b3(%v1), b2 jumps to b1(%v2).
    fn forwarding_params() -> ir::Func<'static> {
        let int = |id| TypeId {
            id: Id(id),
            ty: Type::Int,
        };
        let mut fun = ir::Func::new("f", Id(0), vec![int(0)], Some(Type::Int));
        let b0_params = fun.intern_params([Id(0)]);
        let b1_params = fun.intern_params([Id(1)]);
        let b3_params = fun.intern_params([Id(3)]);
        let into_b1 = fun.intern_params([Id(0)]);
        let b2_into_b1 = fun.intern_params([Id(2)]);
        let into_b3 = fun.intern_params([Id(1)]);
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params: b0_params,
                instructions: vec![],
                term: Some(Terminator::Branch {
                    cond: Id(0),
                    yes: (Id(1), into_b1),
                    no: (Id(2), ir::EMPTY_PARAMS),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params: b1_params,
                instructions: vec![],
                term: Some(Terminator::Jump {
                    id: Id(3),
                    params: into_b3,
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(2),
                params: ir::EMPTY_PARAMS,
                instructions: vec![ir::Instr::LoadConst {
                    dst: int(2),
                    value: ir::constant::Const::Int(7),
                    span: 0,
                }],
                term: Some(Terminator::Jump {
                    id: Id(1),
                    params: b2_into_b1,
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
        fun
    }

    #[test]
    fn forwards_the_branch_args_not_the_skipped_params() {
        let mut fun = forwarding_params();
        indirect_jump(&mut fun);

        let Some(Terminator::Branch {
            yes: (Id(3), args), ..
        }) = fun.blocks[0].term
        else {
            panic!("yes edge should retarget to b3: {:?}", fun.blocks[0].term);
        };
        assert_eq!(fun.params(args), [Id(0)], "%v1 is only bound inside b1");
    }

    #[test]
    fn keeps_a_skipped_block_other_edges_still_reach() {
        let mut fun = forwarding_params();
        indirect_jump(&mut fun);

        assert!(!fun.blocks[1].tombstone, "b2 still jumps into b1");
    }
}
