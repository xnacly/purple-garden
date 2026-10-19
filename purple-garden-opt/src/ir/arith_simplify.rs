use std::alloc::Allocator;

use crate::ir::Scratch;
use purple_garden_ir::{self as ir, BinOp, Id, Instr, constant::Const};

const ONE: u64 = 1.0f64.to_bits();
const TWO: u64 = 2.0f64.to_bits();
const NEG_ZERO: u64 = (-0.0f64).to_bits();

/// Algebraic simplification of binops with a constant or repeated operand, only where the result is
/// the same for every input, NaN and the signed zeros included:
///
/// - `x * 2`&`2 * x` to `x + x` for Int and Double: add is cheaper than imul and drops the constant
/// - `x * 0`&`0 * x` to `0`, Int only: NaN, the infinities and -0.0 break it for Double
/// - `x - x` to `0`, Int only: NaN and the infinities break it for Double
/// - `x == x` to `true`, for the non Double equalities
/// - `x < x`&`x > x` to `false`, for Int and Double: NaN compares false either way
/// - `x * 1`, `1 * x`, `x / 1` to `x`, for Int and Double
/// - `x + 0`, `0 + x`, `x - 0` to `x` for Int; for Double only `x + -0.0`, `-0.0 + x` and
///   `x - 0.0`, since `-0.0 + 0.0` is `0.0`
/// - `x % 1` to `0`, Int only
/// - `x % 2^k == 0` to `x & (2^k - 1) == 0`, Int only, see [`mod_pow2_eq_zero`]
///
/// ```text
/// %v16:Double = 2
/// %v17:Double = DMul %v16, %v0
/// ```
///
/// becomes
///
/// ```text
/// %v16:Double = 2
/// %v17:Double = DAdd %v0, %v0
/// ```
///
/// An `x` result noops the binop and rewrites its uses to `x`. The constants left unused are
/// removed by dce.
pub fn arith_simplify<F: Allocator, S: Allocator>(
    fun: &mut ir::Func<'_, F>,
    scratch: &mut Scratch<'_, S>,
) {
    super::record_uses(fun, scratch);

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

    mod_pow2_eq_zero(fun, scratch);

    for bi in 0..fun.blocks.len() {
        if fun.blocks[bi].tombstone {
            continue;
        }
        for ii in 0..fun.blocks[bi].instructions.len() {
            let instr = &fun.blocks[bi].instructions[ii];
            let Some((simplified, alias)) = simplify(instr, fun, scratch) else {
                continue;
            };

            purple_garden_shared::trace!(
                "[opt::ir::arith_simplify] {:?} to {:?}",
                instr,
                alias.map_or(simplified.clone(), |_| Instr::Noop)
            );

            if let (Some(dst), Some(to)) = (ir::Func::def_of(instr), alias) {
                scratch.alias(dst, to);
            }
            fun.blocks[bi].instructions[ii] = simplified;
        }
    }

    if !scratch.remap().is_empty() {
        fun.replace_uses(scratch.remap());
    }
}

/// `x % 2^k == 0` to `x & (2^k - 1) == 0`: both ask whether the low k bits are zero, the and
/// without the fixup a signed remainder needs. Only for a remainder whose single use is the test,
/// since it differs from the and for negative x, and whose divisor constant has no other use, since
/// that becomes `2^k - 1`.
fn mod_pow2_eq_zero<F: Allocator, S: Allocator>(
    fun: &mut ir::Func<'_, F>,
    scratch: &Scratch<'_, S>,
) {
    let int = |fun: &ir::Func<'_, F>, id| {
        let def = scratch.const_def(id)?;
        match &fun.blocks[def.block as usize].instructions[def.instr as usize] {
            Instr::LoadConst {
                value: Const::Int(v),
                ..
            } => Some(*v),
            _ => None,
        }
    };

    // where each candidate remainder is defined, by its id
    let mut rems = Vec::new_in(scratch.alloc());
    for (bi, block) in fun.blocks.iter().enumerate() {
        if block.tombstone {
            continue;
        }
        for (ii, instr) in block.instructions.iter().enumerate() {
            let Instr::Bin {
                op: BinOp::IMod,
                dst,
                rhs,
                ..
            } = instr
            else {
                continue;
            };
            let Some(divisor) = int(fun, *rhs) else {
                continue;
            };
            if divisor < 2
                || !(divisor as u64).is_power_of_two()
                || scratch.use_count(*rhs) != 1
                || scratch.use_count(dst.id) != 1
            {
                continue;
            }
            let id = dst.id.0 as usize;
            if rems.len() <= id {
                rems.resize(id + 1, None);
            }
            rems[id] = Some((bi, ii));
        }
    }

    for bi in 0..fun.blocks.len() {
        if fun.blocks[bi].tombstone {
            continue;
        }
        for ii in 0..fun.blocks[bi].instructions.len() {
            let Instr::Bin {
                op: BinOp::IEq,
                lhs,
                rhs,
                ..
            } = fun.blocks[bi].instructions[ii]
            else {
                continue;
            };
            let rem = match (int(fun, lhs), int(fun, rhs)) {
                (_, Some(0)) => lhs,
                (Some(0), _) => rhs,
                _ => continue,
            };
            let Some(&Some((rb, ri))) = rems.get(rem.0 as usize) else {
                continue;
            };
            let Instr::Bin { op, rhs, .. } = &mut fun.blocks[rb].instructions[ri] else {
                unreachable!("rems only holds IMods");
            };
            *op = BinOp::IAnd;
            let divisor = scratch
                .const_def(*rhs)
                .expect("rems only holds constant divisors");
            let Instr::LoadConst {
                value: Const::Int(mask),
                ..
            } = &mut fun.blocks[divisor.block as usize].instructions[divisor.instr as usize]
            else {
                unreachable!("rems only holds Int divisors");
            };
            purple_garden_shared::trace!(
                "[opt::ir::arith_simplify] %v{} % {} == 0 to & {}",
                rem.0,
                *mask,
                *mask - 1
            );
            *mask -= 1;
        }
    }
}

fn simplify<'i, F: Allocator, S: Allocator>(
    instr: &Instr<'i>,
    fun: &ir::Func<'i, F>,
    scratch: &Scratch<'_, S>,
) -> Option<(Instr<'i>, Option<Id>)> {
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
    let lhs = &scratch.resolve(*lhs);
    let rhs = &scratch.resolve(*rhs);

    let value = |id| {
        let def = scratch.const_def(id)?;
        let Instr::LoadConst { value, .. } =
            &fun.blocks[def.block as usize].instructions[def.instr as usize]
        else {
            return None;
        };
        Some(value)
    };
    let bin = |op, x| {
        let instr = Instr::Bin {
            op,
            dst: dst.clone(),
            lhs: x,
            rhs: x,
            span: *span,
        };
        (instr, None)
    };
    let load = |value| {
        let instr = Instr::LoadConst {
            dst: dst.clone(),
            value,
            span: *span,
        };
        (instr, None)
    };
    let same = |x| (Instr::Noop, Some(x));

    Some(match (op, value(*lhs), value(*rhs)) {
        (BinOp::IMul, _, Some(Const::Int(2))) => bin(BinOp::IAdd, *lhs),
        (BinOp::IMul, Some(Const::Int(2)), _) => bin(BinOp::IAdd, *rhs),
        (BinOp::DMul, _, Some(Const::Double(TWO))) => bin(BinOp::DAdd, *lhs),
        (BinOp::DMul, Some(Const::Double(TWO)), _) => bin(BinOp::DAdd, *rhs),
        (BinOp::IMul, Some(Const::Int(0)), _) | (BinOp::IMul, _, Some(Const::Int(0))) => {
            load(Const::Int(0))
        }
        (BinOp::IMod, _, Some(Const::Int(1))) => load(Const::Int(0)),
        (BinOp::IMul | BinOp::IDiv, _, Some(Const::Int(1)))
        | (BinOp::IAdd | BinOp::ISub, _, Some(Const::Int(0)))
        | (BinOp::DMul | BinOp::DDiv, _, Some(Const::Double(ONE)))
        | (BinOp::DAdd, _, Some(Const::Double(NEG_ZERO)))
        | (BinOp::DSub, _, Some(Const::Double(0))) => same(*lhs),
        (BinOp::IMul, Some(Const::Int(1)), _)
        | (BinOp::IAdd, Some(Const::Int(0)), _)
        | (BinOp::DMul, Some(Const::Double(ONE)), _)
        | (BinOp::DAdd, Some(Const::Double(NEG_ZERO)), _) => same(*rhs),
        (BinOp::ISub, ..) if lhs == rhs => load(Const::Int(0)),
        (BinOp::IEq | BinOp::BEq | BinOp::SEq, ..) if lhs == rhs => load(Const::True),
        (BinOp::ILt | BinOp::IGt | BinOp::DLt | BinOp::DGt, ..) if lhs == rhs => load(Const::False),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::arith_simplify;
    use crate::ir::Scratch;
    use purple_garden_ir::{
        self as ir, BinOp, Block, Id, Instr, Terminator, TypeId, constant::Const, ptype::Type,
    };

    /// `fn f(x) { lhs <op> rhs }`, a `Some` operand is loaded as a constant, a `None` one is `x`.
    fn run(
        op: BinOp,
        ty: Type<'static>,
        lhs: Option<Const<'static>>,
        rhs: Option<Const<'static>>,
    ) -> ir::Func<'static> {
        let out = match op {
            BinOp::IEq
            | BinOp::BEq
            | BinOp::SEq
            | BinOp::ILt
            | BinOp::IGt
            | BinOp::DLt
            | BinOp::DGt => Type::Bool,
            _ => ty.clone(),
        };
        let mut fun = ir::Func::new(
            "f",
            Id(0),
            vec![TypeId {
                id: Id(0),
                ty: ty.clone(),
            }],
            Some(out.clone()),
        );
        let params = fun.intern_params(vec![Id(0)]);
        let mut instructions = vec![];
        let mut operand = |c: Option<Const<'static>>, id| {
            let Some(value) = c else {
                return Id(0);
            };
            instructions.push(Instr::LoadConst {
                dst: TypeId {
                    id: Id(id),
                    ty: ty.clone(),
                },
                value,
                span: 0,
            });
            Id(id)
        };
        let lhs = operand(lhs, 1);
        let rhs = operand(rhs, 2);
        instructions.push(Instr::Bin {
            op,
            dst: TypeId { id: Id(3), ty: out },
            lhs,
            rhs,
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

        arith_simplify(&mut fun, &mut Scratch::default());
        fun
    }

    /// The simplified Bin of [`run`].
    fn simplify(
        op: BinOp,
        ty: Type<'static>,
        lhs: Option<Const<'static>>,
        rhs: Option<Const<'static>>,
    ) -> Instr<'static> {
        run(op, ty, lhs, rhs).blocks[0].instructions.pop().unwrap()
    }

    fn double(v: f64) -> Option<Const<'static>> {
        Some(Const::Double(v.to_bits()))
    }

    #[test]
    fn mul_by_two_becomes_add() {
        for (op, ty, two, add) in [
            (BinOp::IMul, Type::Int, Some(Const::Int(2)), BinOp::IAdd),
            (BinOp::DMul, Type::Double, double(2.0), BinOp::DAdd),
        ] {
            for (lhs, rhs) in [(None, two.clone()), (two.clone(), None)] {
                assert!(matches!(
                    simplify(op, ty.clone(), lhs, rhs),
                    Instr::Bin { op, lhs: Id(0), rhs: Id(0), .. } if op == add
                ));
            }
        }
    }

    #[test]
    fn int_mul_by_zero_and_self_sub_become_zero() {
        for (op, lhs, rhs) in [
            (BinOp::IMul, None, Some(Const::Int(0))),
            (BinOp::IMul, Some(Const::Int(0)), None),
            (BinOp::ISub, None, None),
        ] {
            assert!(matches!(
                simplify(op, Type::Int, lhs, rhs),
                Instr::LoadConst {
                    value: Const::Int(0),
                    ..
                }
            ));
        }
    }

    #[test]
    fn self_compares_fold() {
        for (op, ty, expected) in [
            (BinOp::IEq, Type::Int, Const::True),
            (BinOp::BEq, Type::Bool, Const::True),
            (BinOp::SEq, Type::Str, Const::True),
            (BinOp::ILt, Type::Int, Const::False),
            (BinOp::IGt, Type::Int, Const::False),
            (BinOp::DLt, Type::Double, Const::False),
            (BinOp::DGt, Type::Double, Const::False),
        ] {
            assert!(matches!(
                simplify(op, ty, None, None),
                Instr::LoadConst { value, .. } if value == expected
            ));
        }
    }

    /// `x * 0.0` is NaN for NaN and the infinities and -0.0 for negative x, `x - x` is NaN for
    /// NaN and the infinities, and `x * 2` with x a constant other than two stays a mul.
    #[test]
    fn leaves_inexact_rewrites_alone() {
        for (op, ty, lhs, rhs) in [
            (BinOp::DMul, Type::Double, None, double(0.0)),
            (BinOp::DSub, Type::Double, None, None),
            (BinOp::DMul, Type::Double, None, double(-2.0)),
            (BinOp::IMul, Type::Int, None, Some(Const::Int(3))),
            (BinOp::IAdd, Type::Int, None, None),
        ] {
            assert!(matches!(
                simplify(op, ty, lhs, rhs),
                Instr::Bin { op: o, .. } if o == op
            ));
        }
    }

    #[test]
    fn identities_return_the_operand() {
        let int = |v| Some(Const::Int(v));
        for (op, ty, lhs, rhs) in [
            (BinOp::IMul, Type::Int, None, int(1)),
            (BinOp::IMul, Type::Int, int(1), None),
            (BinOp::IDiv, Type::Int, None, int(1)),
            (BinOp::IAdd, Type::Int, None, int(0)),
            (BinOp::IAdd, Type::Int, int(0), None),
            (BinOp::ISub, Type::Int, None, int(0)),
            (BinOp::DMul, Type::Double, None, double(1.0)),
            (BinOp::DMul, Type::Double, double(1.0), None),
            (BinOp::DDiv, Type::Double, None, double(1.0)),
            (BinOp::DAdd, Type::Double, None, double(-0.0)),
            (BinOp::DAdd, Type::Double, double(-0.0), None),
            (BinOp::DSub, Type::Double, None, double(0.0)),
        ] {
            let fun = run(op, ty, lhs, rhs);
            assert!(
                matches!(fun.blocks[0].instructions.last(), Some(Instr::Noop)),
                "{op:?}"
            );
            assert!(
                matches!(
                    fun.blocks[0].term,
                    Some(Terminator::Return {
                        value: Some(Id(0)),
                        ..
                    })
                ),
                "{op:?}"
            );
        }
    }

    #[test]
    fn int_mod_one_becomes_zero() {
        assert!(matches!(
            simplify(BinOp::IMod, Type::Int, None, Some(Const::Int(1))),
            Instr::LoadConst {
                value: Const::Int(0),
                ..
            }
        ));
    }

    /// `x % divisor == 0` with the zero on `zero_lhs` side, `reuse` adds another use of the
    /// remainder. Returns the remainder's op and divisor constant.
    fn mod_eq_zero(divisor: i64, zero_lhs: bool, reuse: bool) -> (BinOp, Const<'static>) {
        let ty = |id, ty| TypeId { id: Id(id), ty };
        let mut fun = ir::Func::new("f", Id(0), vec![ty(0, Type::Int)], Some(Type::Bool));
        let params = fun.intern_params(vec![Id(0)]);
        let (lhs, rhs) = if zero_lhs {
            (Id(3), Id(2))
        } else {
            (Id(2), Id(3))
        };
        let mut instructions = vec![
            Instr::LoadConst {
                dst: ty(1, Type::Int),
                value: Const::Int(divisor),
                span: 0,
            },
            Instr::Bin {
                op: BinOp::IMod,
                dst: ty(2, Type::Int),
                lhs: Id(0),
                rhs: Id(1),
                span: 0,
            },
            Instr::LoadConst {
                dst: ty(3, Type::Int),
                value: Const::Int(0),
                span: 0,
            },
            Instr::Bin {
                op: BinOp::IEq,
                dst: ty(4, Type::Bool),
                lhs,
                rhs,
                span: 0,
            },
        ];
        if reuse {
            instructions.push(Instr::Bin {
                op: BinOp::IAdd,
                dst: ty(5, Type::Int),
                lhs: Id(2),
                rhs: Id(2),
                span: 0,
            });
        }
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions,
            term: Some(Terminator::Return {
                value: Some(Id(4)),
                span: 0,
            }),
        }];

        arith_simplify(&mut fun, &mut Scratch::default());
        let (Instr::LoadConst { value, .. }, Instr::Bin { op, .. }) = (
            &fun.blocks[0].instructions[0],
            &fun.blocks[0].instructions[1],
        ) else {
            unreachable!()
        };
        (*op, value.clone())
    }

    #[test]
    fn mod_pow2_eq_zero_becomes_and() {
        for zero_lhs in [false, true] {
            assert!(matches!(
                mod_eq_zero(4, zero_lhs, false),
                (BinOp::IAnd, Const::Int(3))
            ));
        }
    }

    /// `x % 3` has no mask, and `-3 % 2` is -1 where `-3 & 1` is 1, so a remainder used for more
    /// than the zero test stays.
    #[test]
    fn leaves_other_remainders_alone() {
        for (divisor, reuse) in [(3, false), (2, true)] {
            assert!(matches!(
                mod_eq_zero(divisor, false, reuse),
                (BinOp::IMod, Const::Int(d)) if d == divisor
            ));
        }
    }

    /// `-0.0 + 0.0` is `0.0` and `-0.0 - -0.0` is `0.0`, so neither returns x.
    #[test]
    fn leaves_double_zero_identities_with_the_wrong_sign_alone() {
        for (op, lhs, rhs) in [
            (BinOp::DAdd, None, double(0.0)),
            (BinOp::DAdd, double(0.0), None),
            (BinOp::DSub, None, double(-0.0)),
        ] {
            assert!(matches!(
                simplify(op, Type::Double, lhs, rhs),
                Instr::Bin { op: o, .. } if o == op
            ));
        }
    }

    /// `(x * 1) - x`: the sub sees the mul's alias, so it is `x - x` and folds too.
    #[test]
    fn follows_aliases_within_the_pass() {
        let int = |id| TypeId {
            id: Id(id),
            ty: Type::Int,
        };
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
                    value: Const::Int(1),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::IMul,
                    dst: int(2),
                    lhs: Id(0),
                    rhs: Id(1),
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::ISub,
                    dst: int(3),
                    lhs: Id(2),
                    rhs: Id(0),
                    span: 0,
                },
            ],
            term: Some(Terminator::Return {
                value: Some(Id(3)),
                span: 0,
            }),
        }];

        arith_simplify(&mut fun, &mut Scratch::default());

        assert!(matches!(fun.blocks[0].instructions[1], Instr::Noop));
        assert!(matches!(
            fun.blocks[0].instructions[2],
            Instr::LoadConst {
                value: Const::Int(0),
                ..
            }
        ));
    }
}
