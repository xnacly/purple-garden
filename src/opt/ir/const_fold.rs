use crate::ir;

/// constant fold all SSA values consisting fully of constant SSA values.
///
/// for instance
///
/// ```text
/// %v0:Int = 25
/// %v1:Int = 12
/// %v2:Int = IAdd %v0, %v1
/// ```
///
/// Merged into a single integer load definition SSA value
///
/// ```text
/// %v2:Int = 37
/// ```
pub fn const_fold(fun: &mut ir::Func, scratch: &mut super::Scratch) {}

#[cfg(test)]
mod tests {
    use super::const_fold;
    use crate::ir::{self, BinOp, Block, Const, Id, Instr, Terminator, TypeId, ptype::Type};
    use crate::opt::ir::Scratch;

    fn int(id: u32) -> TypeId {
        TypeId {
            id: Id(id),
            ty: Type::Int,
        }
    }

    /// Returns the i64 inside a `LoadConst { Int(_) }`, or panics with
    /// the actual instruction so a failing test prints what it got.
    fn loaded_int(instr: &Instr) -> i64 {
        match instr {
            Instr::LoadConst {
                value: Const::Int(v),
                ..
            } => *v,
            other => panic!("expected LoadConst Int(_), got {:?}", other),
        }
    }

    #[test]
    fn folds_iadd_of_two_constants() {
        // %v0 = 25; %v1 = 12; %v2 = IAdd %v0, %v1   -->   %v2 = 37
        let mut fun = ir::Func::new("f", Id(0), vec![], Some(Type::Int));
        let params = fun.intern_params(vec![]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::LoadConst {
                    dst: int(0),
                    value: Const::Int(25),
                    span: 0,
                },
                Instr::LoadConst {
                    dst: int(1),
                    value: Const::Int(12),
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

        const_fold(&mut fun, &mut Scratch::default());

        assert_eq!(loaded_int(&fun.blocks[0].instructions[2]), 37);
    }

    #[test]
    fn folds_cascading_iadd_in_one_pass() {
        // %v0 = 2; %v1 = 3; %v2 = %v0 + %v1; %v3 = 4; %v4 = %v2 + %v3
        // Forward-walking analysis should pick up %v2 = 5 in time to fold %v4 = 9.
        let mut fun = ir::Func::new("f", Id(0), vec![], Some(Type::Int));
        let params = fun.intern_params(vec![]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::LoadConst {
                    dst: int(0),
                    value: Const::Int(2),
                    span: 0,
                },
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
                Instr::LoadConst {
                    dst: int(3),
                    value: Const::Int(4),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::IAdd,
                    dst: int(4),
                    lhs: Id(2),
                    rhs: Id(3),
                    span: 0,
                },
            ],
            term: Some(Terminator::Return {
                value: Some(Id(4)),
                span: 0,
            }),
        }];

        const_fold(&mut fun, &mut Scratch::default());

        assert_eq!(loaded_int(&fun.blocks[0].instructions[2]), 5);
        assert_eq!(loaded_int(&fun.blocks[0].instructions[4]), 9);
    }

    #[test]
    fn leaves_non_const_operand_alone() {
        // %v0 is a function parameter (not a constant). %v1 = 5.
        // %v2 = IAdd %v0, %v1 must stay a Bin — we can't fold an unknown.
        let mut fun = ir::Func::new("f", Id(0), vec![Id(0)], Some(Type::Int));
        let params = fun.intern_params(vec![Id(0)]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::LoadConst {
                    dst: int(1),
                    value: Const::Int(5),
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

        const_fold(&mut fun, &mut Scratch::default());

        assert!(matches!(
            fun.blocks[0].instructions[1],
            Instr::Bin {
                op: BinOp::IAdd,
                ..
            }
        ));
    }

    #[test]
    fn iadd_wraps_on_overflow() {
        // i64::MAX + 1 must wrap to i64::MIN; the fold has to match the VM's `wrapping_add`
        // behaviour bit-for-bit. If the VM ever switches to checked arithmetic, this test must
        // change too.
        let mut fun = ir::Func::new("f", Id(0), vec![], Some(Type::Int));
        let params = fun.intern_params(vec![]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::LoadConst {
                    dst: int(0),
                    value: Const::Int(i64::MAX),
                    span: 0,
                },
                Instr::LoadConst {
                    dst: int(1),
                    value: Const::Int(1),
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

        const_fold(&mut fun, &mut Scratch::default());

        assert_eq!(loaded_int(&fun.blocks[0].instructions[2]), i64::MIN);
    }

    #[test]
    fn folds_when_constant_input_is_reused() {
        // %v0 = 5; %v1 = 3;
        // %v2 = %v0 + %v1    (= 8)
        // %v3 = %v0 + %v0    (= 10)
        // %v0 has two uses; that doesn't block const-folding; only the
        // single-use safety check in imm_fold cares about that.
        let mut fun = ir::Func::new("f", Id(0), vec![], Some(Type::Int));
        let params = fun.intern_params(vec![]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::LoadConst {
                    dst: int(0),
                    value: Const::Int(5),
                    span: 0,
                },
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
                    op: BinOp::IAdd,
                    dst: int(3),
                    lhs: Id(0),
                    rhs: Id(0),
                    span: 0,
                },
            ],
            term: Some(Terminator::Return {
                value: Some(Id(3)),
                span: 0,
            }),
        }];

        const_fold(&mut fun, &mut Scratch::default());

        assert_eq!(loaded_int(&fun.blocks[0].instructions[2]), 8);
        assert_eq!(loaded_int(&fun.blocks[0].instructions[3]), 10);
    }

    #[test]
    fn folds_full_arithmetic_expression() {
        // 1829182 + 3183192 - 1289 / 312 * 3819289
        //   = (1829182 + 3183192) - ((1289 / 312) * 3819289)
        //   = 5012374 - (4 * 3819289)
        //   = 5012374 - 15277156
        //   = -10264782
        //
        // 9 instructions in, 1 live LoadConst out (plus 8 dead loads).
        // Exercises IAdd + ISub + IMul + IDiv all cascading through the
        // value table in a single forward pass; fails until every arith
        // arm is wired up.
        let mut fun = ir::Func::new("f", Id(0), vec![], Some(Type::Int));
        let params = fun.intern_params(vec![]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::LoadConst {
                    dst: int(0),
                    value: Const::Int(1829182),
                    span: 0,
                },
                Instr::LoadConst {
                    dst: int(1),
                    value: Const::Int(3183192),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::IAdd,
                    dst: int(2),
                    lhs: Id(0),
                    rhs: Id(1),
                    span: 0,
                },
                Instr::LoadConst {
                    dst: int(3),
                    value: Const::Int(1289),
                    span: 0,
                },
                Instr::LoadConst {
                    dst: int(4),
                    value: Const::Int(312),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::IDiv,
                    dst: int(5),
                    lhs: Id(3),
                    rhs: Id(4),
                    span: 0,
                },
                Instr::LoadConst {
                    dst: int(6),
                    value: Const::Int(3819289),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::IMul,
                    dst: int(7),
                    lhs: Id(5),
                    rhs: Id(6),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::ISub,
                    dst: int(8),
                    lhs: Id(2),
                    rhs: Id(7),
                    span: 0,
                },
            ],
            term: Some(Terminator::Return {
                value: Some(Id(8)),
                span: 0,
            }),
        }];

        const_fold(&mut fun, &mut Scratch::default());

        assert_eq!(loaded_int(&fun.blocks[0].instructions[8]), -10264782);
    }
}
