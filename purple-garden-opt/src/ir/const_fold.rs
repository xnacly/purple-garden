use std::alloc::Allocator;

use crate::ir::Scratch;
use purple_garden_ir::{self as ir, BinOp, Instr, TypeId, UnOp, constant::Const, ptype::Type};

/// constant fold all SSA values consisting fully of constant SSA values in a local block
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
pub fn const_fold<F: Allocator, S: Allocator>(
    fun: &mut ir::Func<'_, F>,
    scratch: &mut Scratch<'_, S>,
) {
    let mut changed = false;

    for i in 0..fun.blocks.len() {
        if fun.blocks[i].tombstone {
            continue;
        }

        scratch.reset();

        for j in 0..fun.blocks[i].instructions.len() {
            let (previous, current) = fun.blocks[i].instructions.split_at_mut(j);
            let instr = &mut current[0];
            changed |= try_const_fold(instr, scratch, previous);

            if let Instr::LoadConst {
                dst: TypeId { id, .. },
                ..
            } = instr
            {
                let (block, instr) = (i as u32, j as u32);
                scratch.record_const(*id, block, instr);
            }
        }
    }

    if changed {
        remove_dead_load_consts(fun, scratch);
    }
}

fn remove_dead_load_consts<F: Allocator, S: Allocator>(
    fun: &mut ir::Func<'_, F>,
    scratch: &mut Scratch<'_, S>,
) {
    scratch.reset();

    for block in &fun.blocks {
        if block.tombstone {
            continue;
        }

        for instr in &block.instructions {
            if let Some(id) = ir::Func::def_of(instr) {
                scratch.ensure(id);
            }
            fun.for_each_use_of_instr(instr, |id| scratch.bump(id));
        }

        if let Some(term) = &block.term {
            fun.for_each_use_of_term(term, |id| scratch.bump(id));
        }
    }

    for block in &mut fun.blocks {
        if block.tombstone {
            continue;
        }

        for instr in &mut block.instructions {
            let Instr::LoadConst { dst, .. } = instr else {
                continue;
            };

            if scratch.uses.get(dst.id.0 as usize).copied().unwrap_or(0) == 0 {
                purple_garden_shared::trace!(
                    "[opt::ir::const_fold] removed dead LoadConst %v{}",
                    dst.id.0
                );
                *instr = Instr::Noop;
            }
        }
    }
}

fn try_const_fold<S: Allocator>(
    instr: &mut Instr<'_>,
    scratch: &Scratch<'_, S>,
    previous: &[Instr<'_>],
) -> bool {
    match instr {
        Instr::Cast { dst, from, span } => {
            let Some(def) = scratch.const_def(from.id) else {
                return false;
            };
            let Some(def_value) = const_value(previous, def) else {
                return false;
            };

            let result = match (&from.ty, &dst.ty, def_value) {
                (Type::Double, Type::Int, Const::Double(value)) => {
                    Const::Int(f64::from_bits(*value) as i64)
                }
                (Type::Int, Type::Double, Const::Int(value)) => {
                    Const::Double((*value as f64).to_bits())
                }
                (Type::Int, Type::Bool, Const::Int(value)) => {
                    if *value != 0 {
                        Const::True
                    } else {
                        Const::False
                    }
                }
                (Type::Bool, Type::Int, Const::True) => Const::Int(1),
                (Type::Bool, Type::Int, Const::False) => Const::Int(0),
                _ => return false,
            };

            purple_garden_shared::trace!(
                "[opt::ir::const_fold] folded Cast %v{} into constant {:?} / {}",
                dst.id.0,
                result,
                result
            );
            *instr = Instr::LoadConst {
                dst: dst.clone(),
                value: result,
                span: *span,
            };
            true
        }
        Instr::Bin {
            op,
            dst,
            lhs,
            rhs,
            span,
        } => {
            // we need all binary dependencies to be constant, otherwise we cant fold it and its
            // maybe rather a case for the imm_fold pass
            let (Some(lhs_c), Some(rhs_c)) = (scratch.const_def(*lhs), scratch.const_def(*rhs))
            else {
                return false;
            };
            let (Some(lhs_value), Some(rhs_value)) =
                (const_value(previous, lhs_c), const_value(previous, rhs_c))
            else {
                return false;
            };

            let result: Const = match op {
                BinOp::IAdd => {
                    let (Const::Int(lhs), Const::Int(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    lhs.wrapping_add(*rhs).into()
                }
                BinOp::ISub => {
                    let (Const::Int(lhs), Const::Int(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    lhs.wrapping_sub(*rhs).into()
                }
                BinOp::IMul => {
                    let (Const::Int(lhs), Const::Int(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    lhs.wrapping_mul(*rhs).into()
                }
                BinOp::IDiv => {
                    let (Const::Int(lhs), Const::Int(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    if *rhs == 0 {
                        return false;
                    }
                    (*lhs / *rhs).into()
                }
                BinOp::IMod => {
                    let (Const::Int(lhs), Const::Int(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    if *rhs == 0 {
                        return false;
                    }
                    (*lhs % *rhs).into()
                }
                BinOp::IAnd => {
                    let (Const::Int(lhs), Const::Int(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (lhs & rhs).into()
                }
                BinOp::IOr => {
                    let (Const::Int(lhs), Const::Int(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (lhs | rhs).into()
                }
                BinOp::IXor => {
                    let (Const::Int(lhs), Const::Int(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (lhs ^ rhs).into()
                }
                BinOp::IShl => {
                    let (Const::Int(lhs), Const::Int(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (lhs.wrapping_shl(*rhs as u32)).into()
                }
                BinOp::IShr => {
                    let (Const::Int(lhs), Const::Int(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (lhs.wrapping_shr(*rhs as u32)).into()
                }
                BinOp::IUShr => {
                    let (Const::Int(lhs), Const::Int(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    ((*lhs as u64).wrapping_shr(*rhs as u32) as i64).into()
                }
                BinOp::ILt => {
                    let (Const::Int(lhs), Const::Int(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (lhs < rhs).into()
                }
                BinOp::ILe => {
                    let (Const::Int(lhs), Const::Int(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (lhs <= rhs).into()
                }
                BinOp::IGt => {
                    let (Const::Int(lhs), Const::Int(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (lhs > rhs).into()
                }
                BinOp::IGe => {
                    let (Const::Int(lhs), Const::Int(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (lhs >= rhs).into()
                }
                BinOp::IEq => {
                    let (Const::Int(lhs), Const::Int(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (lhs == rhs).into()
                }
                BinOp::DAdd => {
                    let (Const::Double(lhs), Const::Double(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (f64::from_bits(*lhs) + f64::from_bits(*rhs)).into()
                }
                BinOp::DSub => {
                    let (Const::Double(lhs), Const::Double(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (f64::from_bits(*lhs) - f64::from_bits(*rhs)).into()
                }
                BinOp::DMul => {
                    let (Const::Double(lhs), Const::Double(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (f64::from_bits(*lhs) * f64::from_bits(*rhs)).into()
                }
                BinOp::DDiv => {
                    let (Const::Double(lhs), Const::Double(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (f64::from_bits(*lhs) / f64::from_bits(*rhs)).into()
                }
                BinOp::DLt => {
                    let (Const::Double(lhs), Const::Double(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (f64::from_bits(*lhs) < f64::from_bits(*rhs)).into()
                }
                BinOp::DLe => {
                    let (Const::Double(lhs), Const::Double(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (f64::from_bits(*lhs) <= f64::from_bits(*rhs)).into()
                }
                BinOp::DGt => {
                    let (Const::Double(lhs), Const::Double(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (f64::from_bits(*lhs) > f64::from_bits(*rhs)).into()
                }
                BinOp::DGe => {
                    let (Const::Double(lhs), Const::Double(rhs)) = (lhs_value, rhs_value) else {
                        unreachable!();
                    };
                    (f64::from_bits(*lhs) >= f64::from_bits(*rhs)).into()
                }
                BinOp::BEq | BinOp::SEq => (lhs_value == rhs_value).into(),
            };

            purple_garden_shared::trace!(
                "[opt::ir::const_fold] folded {:?} %v{} into constant {:?} / {}",
                op,
                dst.id.0,
                result,
                result
            );
            *instr = Instr::LoadConst {
                // PERF: is this an issue?
                dst: dst.clone(),
                value: result,
                span: *span,
            };
            true
        }
        // Assert(true) is dead and dumb, remove
        Instr::Un {
            op: UnOp::Assert(_),
            dst,
            src,
            span,
        } => {
            let Some(Const::True) = scratch
                .const_def(*src)
                .and_then(|def| const_value(previous, def))
            else {
                return false;
            };
            *instr = Instr::LoadConst {
                dst: dst.clone(),
                value: Const::Undefined,
                span: *span,
            };
            true
        }
        Instr::Un {
            op, dst, src, span, ..
        } => {
            let Some(value) = scratch
                .const_def(*src)
                .and_then(|def| const_value(previous, def))
            else {
                return false;
            };
            let value = match (*op, value) {
                (UnOp::INot, Const::Int(n)) => (!n).into(),
                (UnOp::IPopcnt, Const::Int(n)) => i64::from(n.count_ones()).into(),
                (UnOp::ICtz, Const::Int(n)) => i64::from(n.trailing_zeros()).into(),
                (UnOp::IClz, Const::Int(n)) => i64::from(n.leading_zeros()).into(),
                (UnOp::DSqrt, Const::Double(n)) => f64::from_bits(*n).sqrt().into(),
                _ => unreachable!("the typechecker typed {op:?}'s operand"),
            };
            *instr = Instr::LoadConst {
                dst: dst.clone(),
                value,
                span: *span,
            };
            true
        }
        // a constant index below the stored length needs no bounds check, the element is a field
        // at a known offset, addrof_fold and load_store_fold take it from there
        Instr::Index {
            dst,
            base,
            idx,
            span,
        } => {
            let Some(Const::Int(idx)) = scratch
                .const_def(*idx)
                .and_then(|def| const_value(previous, def))
            else {
                return false;
            };
            // the length is the array's first word, stored once by its initialisation
            let Some(Const::Int(len)) = previous
                .iter()
                .find_map(|instr| match instr {
                    Instr::Store {
                        src,
                        base: stored,
                        offset: 0,
                        ..
                    } if stored == base => Some(*src),
                    _ => None,
                })
                .and_then(|src| scratch.const_def(src))
                .and_then(|def| const_value(previous, def))
            else {
                return false;
            };
            // out of bounds stays an Index, trapping at runtime
            if !(0..*len).contains(idx) {
                return false;
            }
            let Type::Ptr(element) = dst.ty else {
                unreachable!("Index yields a Ptr to the element, got {}", dst.ty);
            };
            let stride = element.layout().pad_to_align().size() as i64;
            let Some(offset) = idx
                .checked_mul(stride)
                .and_then(|offset| u32::try_from(offset + 8).ok())
            else {
                return false;
            };

            purple_garden_shared::trace!(
                "[opt::ir::const_fold] folded Index %v{} into %v{}+{offset}",
                dst.id.0,
                base.0
            );
            *instr = Instr::AddrOf {
                dst: dst.clone(),
                base: *base,
                offset,
                span: *span,
            };
            true
        }
        // we specifically dont care about BinImm, since imm_fold produces them and runs AFTER this
        // pass
        _ => false,
    }
}

fn const_value<'instr>(
    instructions: &'instr [Instr<'_>],
    def: crate::ir::ConstDef,
) -> Option<&'instr Const<'instr>> {
    let Instr::LoadConst { value, .. } = instructions.get(def.instr as usize)? else {
        return None;
    };
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::const_fold;
    use crate::ir::Scratch;
    use purple_garden_ir::{
        self as ir, BinOp, Block, Check, Id, Instr, Terminator, TypeId, UnOp, constant::Const,
        ptype::Type,
    };

    fn int(id: u32) -> TypeId<'static> {
        TypeId {
            id: Id(id),
            ty: Type::Int,
        }
    }

    fn double(id: u32) -> TypeId<'static> {
        TypeId {
            id: Id(id),
            ty: Type::Double,
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
            other => panic!("expected LoadConst Int(_), got {other:?}"),
        }
    }

    fn loaded_double(instr: &Instr) -> f64 {
        match instr {
            Instr::LoadConst {
                value: Const::Double(v),
                ..
            } => f64::from_bits(*v),
            other => panic!("expected LoadConst Double(_), got {other:?}"),
        }
    }

    fn loaded_bool(instr: &Instr) -> bool {
        match instr {
            Instr::LoadConst {
                value: Const::True, ..
            } => true,
            Instr::LoadConst {
                value: Const::False,
                ..
            } => false,
            other => panic!("expected LoadConst Bool, got {other:?}"),
        }
    }

    /// `%v0 = cond; %v1 = Assert %v0`, the assert after const_fold.
    fn folded_assert(cond: Const<'static>) -> Instr<'static> {
        let mut fun = ir::Func::new("f", Id(0), vec![], Some(Type::Void));
        let params = fun.intern_params(vec![]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::LoadConst {
                    dst: TypeId {
                        id: Id(0),
                        ty: Type::Bool,
                    },
                    value: cond,
                    span: 0,
                },
                Instr::Un {
                    op: UnOp::Assert(Check::InBounds),
                    dst: TypeId {
                        id: Id(1),
                        ty: Type::Void,
                    },
                    src: Id(0),
                    span: 0,
                },
            ],
            term: Some(Terminator::Return {
                value: Some(Id(1)),
                span: 0,
            }),
        }];
        const_fold(&mut fun, &mut Scratch::default());
        fun.blocks[0].instructions[1].clone()
    }

    #[test]
    fn drops_an_assert_that_always_holds() {
        assert!(matches!(
            folded_assert(Const::True),
            Instr::LoadConst {
                value: Const::Undefined,
                ..
            }
        ));
        assert!(matches!(
            folded_assert(Const::False),
            Instr::Un {
                op: UnOp::Assert(_),
                ..
            }
        ));
    }

    /// `%v0 = Alloc; %v1 = len; Store %v0+0, %v1; %v2 = idx; %v3 = Index %v0, %v2`, the Index
    /// after const_fold. Without `len` nothing is stored, the length is unknown.
    fn folded_index(len: Option<i64>, idx: i64, element: Type<'static>) -> Instr<'static> {
        let element: &'static Type<'static> = Box::leak(Box::new(element));
        let mut fun = ir::Func::new("f", Id(0), vec![], Some(Type::Ptr(element)));
        let params = fun.intern_params(vec![]);
        let mut instructions = vec![Instr::Alloc {
            dst: TypeId {
                id: Id(0),
                ty: Type::Array(element),
            },
            layout: std::alloc::Layout::from_size_align(64, 8).unwrap(),
            span: 0,
        }];
        if let Some(len) = len {
            instructions.push(Instr::LoadConst {
                dst: int(1),
                value: Const::Int(len),
                span: 0,
            });
            instructions.push(Instr::Store {
                src: Id(1),
                base: Id(0),
                offset: 0,
                span: 0,
            });
        }
        instructions.push(Instr::LoadConst {
            dst: int(2),
            value: Const::Int(idx),
            span: 0,
        });
        instructions.push(Instr::Index {
            dst: TypeId {
                id: Id(3),
                ty: Type::Ptr(element),
            },
            base: Id(0),
            idx: Id(2),
            span: 0,
        });
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions,
            term: Some(Terminator::Return {
                value: Some(Id(3)),
                span: 0,
            }),
        }];
        const_fold(&mut fun, &mut Scratch::default());
        fun.blocks[0].instructions.last().unwrap().clone()
    }

    #[test]
    fn folds_a_constant_index_in_bounds_into_addrof() {
        assert!(matches!(
            folded_index(Some(3), 2, Type::Int),
            Instr::AddrOf {
                base: Id(0),
                offset: 24,
                ..
            }
        ));
        // the stride is the padded record size
        let record = Type::Record(Box::leak(Box::new([
            ir::ptype::Field {
                name: "x",
                ty: Type::Int,
            },
            ir::ptype::Field {
                name: "y",
                ty: Type::Double,
            },
        ])));
        assert!(matches!(
            folded_index(Some(3), 1, record),
            Instr::AddrOf {
                base: Id(0),
                offset: 24,
                ..
            }
        ));
    }

    #[test]
    fn keeps_an_index_out_of_bounds_or_of_unknown_length() {
        for (len, idx) in [(Some(3), 3), (Some(3), -1), (None, 0)] {
            assert!(
                matches!(folded_index(len, idx, Type::Int), Instr::Index { .. }),
                "len {len:?} idx {idx}"
            );
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
    fn folds_constant_double_to_int_cast() {
        let mut fun = ir::Func::new("f", Id(0), vec![], Some(Type::Int));
        let params = fun.intern_params(vec![]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::LoadConst {
                    dst: double(0),
                    value: Const::Double(3.5f64.to_bits()),
                    span: 0,
                },
                Instr::Cast {
                    dst: int(1),
                    from: double(0),
                    span: 0,
                },
            ],
            term: Some(Terminator::Return {
                value: Some(Id(1)),
                span: 0,
            }),
        }];

        const_fold(&mut fun, &mut Scratch::default());

        assert_eq!(loaded_int(&fun.blocks[0].instructions[1]), 3);
    }

    #[test]
    fn folds_constant_int_to_double_cast() {
        let mut fun = ir::Func::new("f", Id(0), vec![], Some(Type::Double));
        let params = fun.intern_params(vec![]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::LoadConst {
                    dst: int(0),
                    value: Const::Int(42),
                    span: 0,
                },
                Instr::Cast {
                    dst: double(1),
                    from: int(0),
                    span: 0,
                },
            ],
            term: Some(Terminator::Return {
                value: Some(Id(1)),
                span: 0,
            }),
        }];

        const_fold(&mut fun, &mut Scratch::default());

        assert_eq!(loaded_double(&fun.blocks[0].instructions[1]), 42.0);
    }

    #[test]
    fn folds_constant_int_to_bool_cast() {
        let mut fun = ir::Func::new("f", Id(0), vec![], Some(Type::Bool));
        let params = fun.intern_params(vec![]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::LoadConst {
                    dst: int(0),
                    value: Const::Int(42),
                    span: 0,
                },
                Instr::Cast {
                    dst: TypeId {
                        id: Id(1),
                        ty: Type::Bool,
                    },
                    from: int(0),
                    span: 0,
                },
            ],
            term: Some(Terminator::Return {
                value: Some(Id(1)),
                span: 0,
            }),
        }];

        const_fold(&mut fun, &mut Scratch::default());

        assert!(loaded_bool(&fun.blocks[0].instructions[1]));
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

        assert!(matches!(fun.blocks[0].instructions[2], Instr::Noop));
        assert_eq!(loaded_int(&fun.blocks[0].instructions[4]), 9);
    }

    #[test]
    fn leaves_non_const_operand_alone() {
        // %v0 is a function parameter (not a constant). %v1 = 5.
        // %v2 = IAdd %v0, %v1 must stay a Bin; we can't fold an unknown.
        let mut fun = ir::Func::new(
            "f",
            Id(0),
            vec![TypeId {
                id: Id(0),
                ty: Type::Int,
            }],
            Some(Type::Int),
        );
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

        assert!(matches!(fun.blocks[0].instructions[2], Instr::Noop));
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
                    value: Const::Int(1_829_182),
                    span: 0,
                },
                Instr::LoadConst {
                    dst: int(1),
                    value: Const::Int(3_183_192),
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
                    value: Const::Int(3_819_289),
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

        assert_eq!(loaded_int(&fun.blocks[0].instructions[8]), -10_264_782);
    }

    #[test]
    fn leaves_idiv_by_zero_alone() {
        let mut fun = ir::Func::new("f", Id(0), vec![], Some(Type::Int));
        let params = fun.intern_params(vec![]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::LoadConst {
                    dst: int(0),
                    value: Const::Int(1),
                    span: 0,
                },
                Instr::LoadConst {
                    dst: int(1),
                    value: Const::Int(0),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::IDiv,
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
            fun.blocks[0].instructions[2],
            Instr::Bin {
                op: BinOp::IDiv,
                ..
            }
        ));
    }

    #[test]
    fn folds_ddiv_by_zero_to_inf() {
        let mut fun = ir::Func::new("f", Id(0), vec![], Some(Type::Double));
        let params = fun.intern_params(vec![]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::LoadConst {
                    dst: double(0),
                    value: Const::Double(1.0f64.to_bits()),
                    span: 0,
                },
                Instr::LoadConst {
                    dst: double(1),
                    value: Const::Double(0.0f64.to_bits()),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::DDiv,
                    dst: double(2),
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
            fun.blocks[0].instructions[2],
            Instr::LoadConst {
                value: Const::Double(bits),
                ..
            } if f64::from_bits(bits) == f64::INFINITY
        ));
    }
}
