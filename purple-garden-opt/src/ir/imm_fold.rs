use crate::ir::Scratch;
use purple_garden_ir::{self as ir, BinOp, Id, Instr, TypeId, constant::Const};

/// Fold single-use integer constants into integer binops while the IR
/// still knows SSA use counts.
pub fn imm_fold<'fun, 's>(fun: &'fun mut ir::Func<'s>, scratch: &mut super::Scratch<'s>) {
    scratch.reset();

    for (bi, block) in fun.blocks.iter().enumerate() {
        if block.tombstone {
            continue;
        }
        for (ii, instr) in block.instructions.iter().enumerate() {
            if let Instr::LoadConst { dst, .. } = instr {
                scratch.record_const(dst.id, bi as u32, ii as u32);
            }
        }
    }

    for block in &fun.blocks {
        if block.tombstone {
            continue;
        }
        for instr in &block.instructions {
            ir::Func::for_each_use_of_instr(instr, |id| bump_if_const(scratch, id));
        }
        if let Some(term) = &block.term {
            fun.for_each_use_of_term(term, |id| bump_if_const(scratch, id));
        }
    }

    for bi in 0..fun.blocks.len() {
        if fun.blocks[bi].tombstone {
            continue;
        }
        for ii in 0..fun.blocks[bi].instructions.len() {
            if let Some((op, lhs, _def_block, _def_instr, imm, dst, span)) =
                try_fold(&fun.blocks[bi].instructions[ii], scratch, fun)
            {
                purple_garden_shared::trace!(
                    "[opt::ir::imm_fold] folded constant {} into immediate {:?}",
                    imm,
                    op
                );

                fun.blocks[bi].instructions[ii] = Instr::BinImm {
                    op,
                    dst,
                    lhs,
                    imm,
                    span,
                };
                continue;
            }

            let Some((op, lhs, bits, dst, span)) =
                try_fold_double(&fun.blocks[bi].instructions[ii], scratch, fun)
            else {
                continue;
            };

            purple_garden_shared::trace!(
                "[opt::ir::imm_fold] folded constant {} into immediate {:?}",
                f64::from_bits(bits),
                op
            );

            fun.blocks[bi].instructions[ii] = Instr::BinImmD {
                op,
                dst,
                lhs,
                bits,
                span,
            };
        }
    }
}

fn bump_if_const(scratch: &mut Scratch<'_>, id: Id) {
    // imm_fold only cares whether recorded LoadConst defs are single-use, so
    // avoid growing the scratch vectors for arbitrary non-constant ids.
    let idx = id.0 as usize;
    if scratch.consts.get(idx).is_some_and(Option::is_some) {
        scratch.uses[idx] += 1;
    }
}

fn try_fold<'scratch>(
    instr: &Instr<'scratch>,
    scratch: &Scratch<'_>,
    fun: &ir::Func<'scratch>,
) -> Option<(BinOp, Id, u32, u32, i32, TypeId<'scratch>, u32)> {
    let Instr::Bin {
        op,
        dst,
        lhs,
        rhs,
        span,
    } = instr
    else {
        return None;
    };

    let lhs_c = scratch.single_use_const(*lhs);
    let rhs_c = scratch.single_use_const(*rhs);

    let (new_op, new_lhs, def) = match op {
        BinOp::IEq | BinOp::IAdd | BinOp::IMul => match (rhs_c, lhs_c) {
            (Some(d), _) => (op.clone(), *lhs, d),
            (None, Some(d)) => (op.clone(), *rhs, d),
            _ => return None,
        },
        BinOp::IGt => match (rhs_c, lhs_c) {
            (Some(d), _) => (BinOp::IGt, *lhs, d),
            (None, Some(d)) => (BinOp::ILt, *rhs, d),
            _ => return None,
        },
        BinOp::ILt => match (rhs_c, lhs_c) {
            (Some(d), _) => (BinOp::ILt, *lhs, d),
            (None, Some(d)) => (BinOp::IGt, *rhs, d),
            _ => return None,
        },
        BinOp::ISub => (BinOp::ISub, *lhs, rhs_c?),
        BinOp::IDiv => (BinOp::IDiv, *lhs, rhs_c?),
        BinOp::IMod => (BinOp::IMod, *lhs, rhs_c?),
        _ => return None,
    };

    let Const::Int(value) = const_value(fun, def)? else {
        return None;
    };

    // Bytecode immediate ops carry an i32; bail if the constant doesn't
    // fit; the original Bin + LoadConst stay intact and run as-is.
    let imm = i32::try_from(*value).ok()?;

    Some((
        new_op,
        new_lhs,
        def.block,
        def.instr,
        imm,
        dst.clone(),
        *span,
    ))
}

fn try_fold_double<'scratch>(
    instr: &Instr<'scratch>,
    scratch: &Scratch<'_>,
    fun: &ir::Func<'scratch>,
) -> Option<(BinOp, Id, u64, TypeId<'scratch>, u32)> {
    let Instr::Bin {
        op,
        dst,
        lhs,
        rhs,
        span,
    } = instr
    else {
        return None;
    };

    let lhs_c = scratch.single_use_const(*lhs);
    let rhs_c = scratch.single_use_const(*rhs);

    let (new_op, new_lhs, def) = match op {
        BinOp::DAdd | BinOp::DMul => match (rhs_c, lhs_c) {
            (Some(d), _) => (*op, *lhs, d),
            (None, Some(d)) => (*op, *rhs, d),
            _ => return None,
        },
        BinOp::DGt => match (rhs_c, lhs_c) {
            (Some(d), _) => (BinOp::DGt, *lhs, d),
            (None, Some(d)) => (BinOp::DLt, *rhs, d),
            _ => return None,
        },
        BinOp::DLt => match (rhs_c, lhs_c) {
            (Some(d), _) => (BinOp::DLt, *lhs, d),
            (None, Some(d)) => (BinOp::DGt, *rhs, d),
            _ => return None,
        },
        BinOp::DSub => (BinOp::DSub, *lhs, rhs_c?),
        BinOp::DDiv => (BinOp::DDiv, *lhs, rhs_c?),
        _ => return None,
    };

    let Const::Double(bits) = const_value(fun, def)? else {
        return None;
    };
    let bits = *bits;

    if matches!(new_op, BinOp::DDiv) && f64::from_bits(bits) == 0.0 {
        return None;
    }

    Some((new_op, new_lhs, bits, dst.clone(), *span))
}

fn const_value<'fun>(
    fun: &'fun ir::Func<'_>,
    def: crate::ir::ConstDef,
) -> Option<&'fun Const<'fun>> {
    let Instr::LoadConst { value, .. } = fun
        .blocks
        .get(def.block as usize)?
        .instructions
        .get(def.instr as usize)?
    else {
        return None;
    };
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::imm_fold;
    use crate::ir::Scratch;
    use purple_garden_ir::{
        self as ir, BinOp, Block, Id, Instr, Terminator, TypeId, constant::Const, ptype::Type,
    };

    fn int(id: u32) -> TypeId<'static> {
        TypeId {
            id: Id(id),
            ty: Type::Int,
        }
    }

    fn dbl(id: u32) -> TypeId<'static> {
        TypeId {
            id: Id(id),
            ty: Type::Double,
        }
    }

    fn double_fun(instructions: Vec<Instr<'static>>, ret: Id) -> ir::Func<'static> {
        let mut fun = ir::Func::new("f", Id(0), vec![Id(0)], Some(Type::Double));
        let params = fun.intern_params(vec![Id(0)]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions,
            term: Some(Terminator::Return {
                value: Some(ret),
                span: 0,
            }),
        }];
        fun
    }

    fn double_const(id: u32, value: f64) -> Instr<'static> {
        Instr::LoadConst {
            dst: dbl(id),
            value: Const::Double(value.to_bits()),
            span: 0,
        }
    }

    #[test]
    fn folds_single_use_rhs_double_const() {
        let mut fun = double_fun(
            vec![
                double_const(1, 2.5),
                Instr::Bin {
                    op: BinOp::DMul,
                    dst: dbl(2),
                    lhs: Id(0),
                    rhs: Id(1),
                    span: 0,
                },
            ],
            Id(2),
        );

        imm_fold(&mut fun, &mut Scratch::default());

        let Instr::BinImmD { op, lhs, bits, .. } = &fun.blocks[0].instructions[1] else {
            panic!("expected BinImmD, got {:?}", fun.blocks[0].instructions[1]);
        };
        assert_eq!(*op, BinOp::DMul);
        assert_eq!(*lhs, Id(0));
        assert_eq!(f64::from_bits(*bits), 2.5);
    }

    #[test]
    fn folds_commutative_lhs_double_const() {
        let mut fun = double_fun(
            vec![
                double_const(1, 3.0),
                Instr::Bin {
                    op: BinOp::DAdd,
                    dst: dbl(2),
                    lhs: Id(1),
                    rhs: Id(0),
                    span: 0,
                },
            ],
            Id(2),
        );

        imm_fold(&mut fun, &mut Scratch::default());

        let Instr::BinImmD { op, lhs, bits, .. } = &fun.blocks[0].instructions[1] else {
            panic!("expected BinImmD, got {:?}", fun.blocks[0].instructions[1]);
        };
        assert_eq!(*op, BinOp::DAdd);
        assert_eq!(*lhs, Id(0));
        assert_eq!(f64::from_bits(*bits), 3.0);
    }

    #[test]
    fn swaps_lhs_double_compare_const() {
        let mut fun = double_fun(
            vec![
                double_const(1, 4.0),
                Instr::Bin {
                    op: BinOp::DGt,
                    dst: TypeId {
                        id: Id(2),
                        ty: Type::Bool,
                    },
                    lhs: Id(1),
                    rhs: Id(0),
                    span: 0,
                },
            ],
            Id(2),
        );

        imm_fold(&mut fun, &mut Scratch::default());

        let Instr::BinImmD { op, lhs, bits, .. } = &fun.blocks[0].instructions[1] else {
            panic!("expected BinImmD, got {:?}", fun.blocks[0].instructions[1]);
        };
        assert_eq!(*op, BinOp::DLt);
        assert_eq!(*lhs, Id(0));
        assert_eq!(f64::from_bits(*bits), 4.0);
    }

    #[test]
    fn leaves_non_commutative_lhs_double_const_alone() {
        let mut fun = double_fun(
            vec![
                double_const(1, 5.0),
                Instr::Bin {
                    op: BinOp::DSub,
                    dst: dbl(2),
                    lhs: Id(1),
                    rhs: Id(0),
                    span: 0,
                },
            ],
            Id(2),
        );

        imm_fold(&mut fun, &mut Scratch::default());

        assert!(matches!(fun.blocks[0].instructions[1], Instr::Bin { .. }));
    }

    #[test]
    fn leaves_double_division_by_zero_alone() {
        for divisor in [0.0_f64, -0.0_f64] {
            let mut fun = double_fun(
                vec![
                    double_const(1, divisor),
                    Instr::Bin {
                        op: BinOp::DDiv,
                        dst: dbl(2),
                        lhs: Id(0),
                        rhs: Id(1),
                        span: 0,
                    },
                ],
                Id(2),
            );

            imm_fold(&mut fun, &mut Scratch::default());

            assert!(
                matches!(fun.blocks[0].instructions[1], Instr::Bin { .. }),
                "divisor {divisor} must keep the trapping DDiv path"
            );
        }
    }

    #[test]
    fn folds_nonzero_double_divisor() {
        let mut fun = double_fun(
            vec![
                double_const(1, 100.0),
                Instr::Bin {
                    op: BinOp::DDiv,
                    dst: dbl(2),
                    lhs: Id(0),
                    rhs: Id(1),
                    span: 0,
                },
            ],
            Id(2),
        );

        imm_fold(&mut fun, &mut Scratch::default());

        assert!(matches!(
            fun.blocks[0].instructions[1],
            Instr::BinImmD {
                op: BinOp::DDiv,
                ..
            }
        ));
    }

    #[test]
    fn folds_both_double_constants_of_a_mandelbrot_step() {
        let mut fun = ir::Func::new("mandel", Id(0), vec![Id(0), Id(1)], Some(Type::Double));
        let params = fun.intern_params(vec![Id(0), Id(1)]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::Bin {
                    op: BinOp::DMul,
                    dst: dbl(2),
                    lhs: Id(0),
                    rhs: Id(0),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::DMul,
                    dst: dbl(3),
                    lhs: Id(1),
                    rhs: Id(1),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::DAdd,
                    dst: dbl(4),
                    lhs: Id(2),
                    rhs: Id(3),
                    span: 0,
                },
                double_const(5, 4.0),
                Instr::Bin {
                    op: BinOp::DGt,
                    dst: TypeId {
                        id: Id(6),
                        ty: Type::Bool,
                    },
                    lhs: Id(4),
                    rhs: Id(5),
                    span: 0,
                },
                double_const(7, 2.0),
                Instr::Bin {
                    op: BinOp::DMul,
                    dst: dbl(8),
                    lhs: Id(7),
                    rhs: Id(0),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::DMul,
                    dst: dbl(9),
                    lhs: Id(8),
                    rhs: Id(1),
                    span: 0,
                },
            ],
            term: Some(Terminator::Return {
                value: Some(Id(9)),
                span: 0,
            }),
        }];

        imm_fold(&mut fun, &mut Scratch::default());

        let folded: Vec<(BinOp, Id, f64)> = fun.blocks[0]
            .instructions
            .iter()
            .filter_map(|instr| match instr {
                Instr::BinImmD { op, lhs, bits, .. } => Some((*op, *lhs, f64::from_bits(*bits))),
                _ => None,
            })
            .collect();

        assert_eq!(
            folded,
            vec![(BinOp::DGt, Id(4), 4.0), (BinOp::DMul, Id(0), 2.0)],
            "both double constants of the step should fold, got {:?}",
            fun.blocks[0].instructions
        );

        let register_binops = fun.blocks[0]
            .instructions
            .iter()
            .filter(|instr| matches!(instr, Instr::Bin { .. }))
            .count();
        assert_eq!(
            register_binops, 4,
            "the constant-free binops must keep their register form"
        );
    }

    #[test]
    fn leaves_multi_use_double_const_alone() {
        let mut fun = double_fun(
            vec![
                double_const(1, 2.0),
                Instr::Bin {
                    op: BinOp::DMul,
                    dst: dbl(2),
                    lhs: Id(0),
                    rhs: Id(1),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::DAdd,
                    dst: dbl(3),
                    lhs: Id(2),
                    rhs: Id(1),
                    span: 0,
                },
            ],
            Id(3),
        );

        imm_fold(&mut fun, &mut Scratch::default());

        assert!(matches!(fun.blocks[0].instructions[1], Instr::Bin { .. }));
        assert!(matches!(fun.blocks[0].instructions[2], Instr::Bin { .. }));
    }

    #[test]
    fn folds_single_use_rhs_const() {
        let mut fun = ir::Func::new("f", Id(0), vec![Id(0)], Some(Type::Int));
        let params = fun.intern_params(vec![Id(0)]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::LoadConst {
                    dst: int(1),
                    value: Const::Int(3),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::IAdd,
                    dst: int(2),
                    lhs: Id(0),
                    rhs: Id(1),
                    span: 0,
                },
            ],
            term: Some(Terminator::Return {
                value: Some(Id(2)),
                span: 0,
            }),
        }];

        imm_fold(&mut fun, &mut Scratch::default());

        assert!(matches!(
            fun.blocks[0].instructions[0],
            Instr::LoadConst { .. }
        ));
        assert!(matches!(
            &fun.blocks[0].instructions[1],
            Instr::BinImm {
                op: BinOp::IAdd,
                lhs: Id(0),
                imm: 3,
                ..
            }
        ));
    }

    #[test]
    fn leaves_multi_use_const_alone() {
        let mut fun = ir::Func::new("f", Id(0), vec![Id(0)], Some(Type::Int));
        let params = fun.intern_params(vec![Id(0)]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::LoadConst {
                    dst: int(1),
                    value: Const::Int(3),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::IAdd,
                    dst: int(2),
                    lhs: Id(0),
                    rhs: Id(1),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::IMul,
                    dst: int(3),
                    lhs: Id(2),
                    rhs: Id(1),
                    span: 0,
                },
            ],
            term: Some(Terminator::Return {
                value: Some(Id(3)),
                span: 0,
            }),
        }];

        imm_fold(&mut fun, &mut Scratch::default());

        assert!(matches!(
            fun.blocks[0].instructions[0],
            Instr::LoadConst { .. }
        ));
        assert!(matches!(fun.blocks[0].instructions[1], Instr::Bin { .. }));
        assert!(matches!(fun.blocks[0].instructions[2], Instr::Bin { .. }));
    }

    #[test]
    fn swaps_lhs_compare_const() {
        let mut fun = ir::Func::new("f", Id(0), vec![Id(0)], Some(Type::Bool));
        let params = fun.intern_params(vec![Id(0)]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::LoadConst {
                    dst: int(1),
                    value: Const::Int(7),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::IGt,
                    dst: TypeId {
                        id: Id(2),
                        ty: Type::Bool,
                    },
                    lhs: Id(1),
                    rhs: Id(0),
                    span: 0,
                },
            ],
            term: Some(Terminator::Return {
                value: Some(Id(2)),
                span: 0,
            }),
        }];

        imm_fold(&mut fun, &mut Scratch::default());

        assert!(matches!(
            &fun.blocks[0].instructions[1],
            Instr::BinImm {
                op: BinOp::ILt,
                lhs: Id(0),
                imm: 7,
                ..
            }
        ));
    }
}
