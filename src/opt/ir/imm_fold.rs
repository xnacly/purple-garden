use crate::ir::{self, BinOp, Const, Id, Instr, TypeId};

#[derive(Default)]
pub struct Scratch {
    uses: Vec<u32>,
    consts: Vec<Option<ConstDef>>,
}

#[derive(Clone, Copy)]
struct ConstDef {
    value: i32,
    block: u32,
    instr: u32,
}

/// Fold single-use integer constants into integer binops while the IR
/// still knows SSA use counts.
pub fn imm_fold(fun: &mut ir::Func, scratch: &mut Scratch) {
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
                if let Ok(value) = i32::try_from(*v) {
                    scratch.ensure(dst.id);
                    scratch.consts[dst.id.0 as usize] = Some(ConstDef {
                        value,
                        block: bi as u32,
                        instr: ii as u32,
                    });
                }
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
            let Some((op, lhs, def, dst, span)) = try_fold(&fun.blocks[bi].instructions[ii], scratch)
            else {
                continue;
            };

            opt_trace!(
                "ir::imm_fold",
                format!("folded constant {} into immediate {:?}", def.value, op)
            );

            fun.blocks[bi].instructions[ii] = Instr::BinImm {
                op,
                dst,
                lhs,
                imm: def.value,
                span,
            };
            fun.blocks[def.block as usize].instructions[def.instr as usize] = Instr::Noop;
        }
    }
}

impl Scratch {
    fn ensure(&mut self, id: Id) {
        let len = id.0 as usize + 1;
        if self.uses.len() < len {
            self.uses.resize(len, 0);
            self.consts.resize(len, None);
        }
    }

    fn bump(&mut self, id: Id) {
        self.ensure(id);
        self.uses[id.0 as usize] += 1;
    }

    fn single_use_const(&self, id: Id) -> Option<ConstDef> {
        let idx = id.0 as usize;
        if self.uses.get(idx).copied() != Some(1) {
            return None;
        }
        self.consts[idx]
    }
}

fn try_fold(
    instr: &Instr<'_>,
    scratch: &Scratch,
) -> Option<(BinOp, Id, ConstDef, TypeId, u32)> {
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

    Some((new_op, new_lhs, def, dst.clone(), *span))
}

#[cfg(test)]
mod tests {
    use super::{imm_fold, Scratch};
    use crate::ir::{self, ptype::Type, BinOp, Block, Const, Id, Instr, Terminator, TypeId};

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
