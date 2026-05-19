use crate::{
    ir::{self, BinOp, Const, Id, Instr, TypeId},
    opt::ir::{ConstDef, Scratch},
};

/// Fold single-use integer constants into integer binops while the IR
/// still knows SSA use counts.
pub fn imm_fold(fun: &mut ir::Func, scratch: &mut super::Scratch) {
    scratch.uses.clear();
    scratch.consts.clear();

    for (bi, block) in fun.blocks.iter().enumerate() {
        if block.tombstone {
            continue;
        }
        for (ii, instr) in block.instructions.iter().enumerate() {
            if let Instr::LoadConst {
                dst,
                value: Const::Int(v),
                ..
            } = instr
            {
                scratch.ensure(dst.id);
                scratch.consts[dst.id.0 as usize] = Some(ConstDef {
                    value: *v,
                    block: bi as u32,
                    instr: ii as u32,
                });
            }
            ir::Func::for_each_use_of_instr(instr, |id| scratch.bump(id));
        }
        if let Some(term) = &block.term {
            fun.for_each_use_of_term(term, |id| scratch.bump(id));
        }
    }

    for bi in 0..fun.blocks.len() {
        if fun.blocks[bi].tombstone {
            continue;
        }
        for ii in 0..fun.blocks[bi].instructions.len() {
            let Some((op, lhs, def, imm, dst, span)) =
                try_fold(&fun.blocks[bi].instructions[ii], scratch)
            else {
                continue;
            };

            opt_trace!(
                "ir::imm_fold",
                format!("folded constant {} into immediate {:?}", imm, op)
            );

            fun.blocks[bi].instructions[ii] = Instr::BinImm {
                op,
                dst,
                lhs,
                imm,
                span,
            };
            fun.blocks[def.block as usize].instructions[def.instr as usize] = Instr::Noop;
        }
    }
}

fn try_fold(
    instr: &Instr<'_>,
    scratch: &Scratch,
) -> Option<(BinOp, Id, ConstDef, i32, TypeId, u32)> {
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
        _ => return None,
    };

    // Bytecode immediate ops carry an i32; bail if the constant doesn't
    // fit; the original Bin + LoadConst stay intact and run as-is.
    let imm = i32::try_from(def.value).ok()?;

    Some((new_op, new_lhs, def, imm, dst.clone(), *span))
}

#[cfg(test)]
mod tests {
    use super::imm_fold;
    use crate::ir::{self, BinOp, Block, Const, Id, Instr, Terminator, TypeId, ptype::Type};
    use crate::opt::ir::Scratch;

    fn int(id: u32) -> TypeId {
        TypeId {
            id: Id(id),
            ty: Type::Int,
        }
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

        assert!(matches!(fun.blocks[0].instructions[0], Instr::Noop));
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
