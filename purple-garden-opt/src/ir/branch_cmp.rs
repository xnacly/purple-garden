use purple_garden_ir::{self as ir, BinOp, Instr, Terminator};

/// Fuse `IEq`-with-immediate branch conditions into `BranchCmpImm`.
///
/// This removes the boolean-producing `BinImm`, so it only fires when the
/// branch is the condition's sole use. Backends can then lower the terminator as
/// a direct compare-and-branch instead of materializing `0` or `1` first.
pub fn branch_cmp(fun: &mut ir::Func<'_>, scratch: &mut super::Scratch<'_>) {
    super::record_uses(fun, scratch);

    for block in &mut fun.blocks {
        if block.tombstone {
            continue;
        }

        let Some(Terminator::Branch {
            cond,
            yes,
            no,
            span,
        }) = block.term
        else {
            continue;
        };

        // The replacement noops the defining `BinImm`. If the boolean is used
        // anywhere else, that other user still needs the materialized value.
        if scratch.use_count(cond) != 1 {
            continue;
        }

        let Some((instr_idx, lhs, imm)) =
            block
                .instructions
                .iter()
                .enumerate()
                .find_map(|(idx, instr)| match instr {
                    Instr::BinImm {
                        op: BinOp::IEq,
                        dst,
                        lhs,
                        imm,
                        ..
                    } if dst.id == cond => Some((idx, *lhs, *imm)),
                    _ => None,
                })
        else {
            continue;
        };

        purple_garden_shared::trace!(
            "[opt::ir::branch_cmp] folded %v{} = IEq %v{}, {} into b{} BranchCmpImm",
            cond.0,
            lhs.0,
            imm,
            block.id.0
        );

        block.instructions[instr_idx] = Instr::Noop;
        block.term = Some(Terminator::BranchCmpImm {
            op: BinOp::IEq,
            lhs,
            imm,
            yes,
            no,
            span,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::branch_cmp;
    use purple_garden_ir::{
        BinOp, Block, EMPTY_PARAMS, Func, Id, Instr, Terminator, TypeId, ptype::Type,
    };

    #[test]
    fn folds_single_use_ieq_imm_branch_condition() {
        let mut scratch = super::super::Scratch::default();
        let mut fun = Func::new("f", Id(0), vec![Id(0)], None);
        let params = fun.intern_params(vec![Id(0)]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![Instr::BinImm {
                op: BinOp::IEq,
                dst: TypeId {
                    id: Id(1),
                    ty: Type::Bool,
                },
                lhs: Id(0),
                imm: 0,
                span: 0,
            }],
            term: Some(Terminator::Branch {
                cond: Id(1),
                yes: (Id(0), EMPTY_PARAMS),
                no: (Id(0), EMPTY_PARAMS),
                span: 0,
            }),
        }];

        branch_cmp(&mut fun, &mut scratch);

        assert!(matches!(fun.blocks[0].instructions[0], Instr::Noop));
        assert!(matches!(
            fun.blocks[0].term,
            Some(Terminator::BranchCmpImm {
                op: BinOp::IEq,
                lhs: Id(0),
                imm: 0,
                ..
            })
        ));
    }

    #[test]
    fn leaves_multi_use_ieq_imm_branch_condition() {
        let mut scratch = super::super::Scratch::default();
        let mut fun = Func::new("f", Id(0), vec![Id(0)], None);
        let params = fun.intern_params(vec![Id(0), Id(1)]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![Instr::BinImm {
                op: BinOp::IEq,
                dst: TypeId {
                    id: Id(1),
                    ty: Type::Bool,
                },
                lhs: Id(0),
                imm: 0,
                span: 0,
            }],
            term: Some(Terminator::Branch {
                cond: Id(1),
                yes: (Id(0), params),
                no: (Id(0), EMPTY_PARAMS),
                span: 0,
            }),
        }];

        branch_cmp(&mut fun, &mut scratch);

        assert!(matches!(
            fun.blocks[0].instructions[0],
            Instr::BinImm {
                op: BinOp::IEq,
                dst: TypeId { id: Id(1), .. },
                lhs: Id(0),
                imm: 0,
                ..
            }
        ));
        assert!(matches!(
            fun.blocks[0].term,
            Some(Terminator::Branch { cond: Id(1), .. })
        ));
    }
}
