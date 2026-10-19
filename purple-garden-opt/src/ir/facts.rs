use std::alloc::Allocator;

use purple_garden_ir as ir;

/// Records what holds for the params of `fun`'s blocks in [`ir::Func::facts`], from the branches
/// leading to them. The last pass, nothing changes the IR after it, so the facts stay valid for the
/// backends and the display. In `b2` below its param `%v1` is non-zero, `b2` is only reached
/// through the branch's no edge:
///
/// ```text
/// b0(%v0, %v1):
///     br_imm IEq %v1, 0, b1(%v0, %v1), b2(%v0, %v1)
/// b2(%v0, %v1): [%v1 nonzero]
///     %v5:Int = IMod %v0, %v1      // the jit skips the zero check
/// ```
pub fn facts<F: Allocator>(fun: &mut ir::Func<'_, F>) {
    fun.facts.clear();
    // TODO(xnacly): a `BranchCmpImm { op: IEq, imm: 0, lhs, no: (block, args), .. }` whose `no`
    //   block has no other predecessor (`super::predecessor_counts`): every arg that is `lhs`
    //   makes the block's param at that position non-zero,
    //   `fun.facts.add_param(block, i, Fact::NONZERO)`
}

#[cfg(test)]
mod tests {
    use super::facts;
    use purple_garden_ir::{
        self as ir, BinOp, Block, Fact, Id, Instr, Terminator, TypeId, constant::Const, ptype::Type,
    };

    fn int(id: u32) -> TypeId<'static> {
        TypeId {
            id: Id(id),
            ty: Type::Int,
        }
    }

    fn ret(id: u32) -> Option<Terminator> {
        Some(Terminator::Return {
            value: Some(Id(id)),
            span: 0,
        })
    }

    /// `gcd(a b)`: `b0` branches on `b == 0`, `b2` is only reached when it isn't and divides by it.
    /// With `shared`, `b2` also gets a jump from `b1`, so `b` may be zero there.
    fn gcd(shared: bool) -> ir::Func<'static> {
        let mut fun = ir::Func::new("gcd", Id(1), vec![int(0), int(1)], Some(Type::Int));
        let params = fun.intern_params([Id(0), Id(1)]);
        let b1_term = if shared {
            Some(Terminator::Jump {
                id: Id(2),
                params,
                span: 0,
            })
        } else {
            ret(0)
        };
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params,
                instructions: vec![],
                term: Some(Terminator::BranchCmpImm {
                    op: BinOp::IEq,
                    lhs: Id(1),
                    imm: 0,
                    yes: (Id(1), params),
                    no: (Id(2), params),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params,
                instructions: vec![],
                term: b1_term,
            },
            Block {
                tombstone: false,
                id: Id(2),
                params,
                instructions: vec![Instr::Bin {
                    op: BinOp::IMod,
                    dst: int(5),
                    lhs: Id(0),
                    rhs: Id(1),
                    span: 0,
                }],
                term: ret(5),
            },
        ];
        fun
    }

    #[test]
    #[ignore = "facts is not implemented yet"]
    fn a_branch_proves_its_no_block_nonzero() {
        let mut fun = gcd(false);
        facts(&mut fun);
        assert!(fun.fact_holds(Id(2), Id(1), Fact::NONZERO));
        assert!(!fun.fact_holds(Id(1), Id(1), Fact::NONZERO));
        assert!(!fun.fact_holds(Id(0), Id(1), Fact::NONZERO));
    }

    /// `b2` is also reached from `b1`, where `b` is zero.
    #[test]
    fn a_block_with_other_predecessors_proves_nothing() {
        let mut fun = gcd(true);
        facts(&mut fun);
        assert!(!fun.fact_holds(Id(2), Id(1), Fact::NONZERO));
    }

    /// The pass starts over, facts from an earlier run don't survive.
    #[test]
    fn clears_earlier_facts() {
        let mut fun = gcd(true);
        fun.facts.add_param(Id(2), 1, Fact::NONZERO);
        facts(&mut fun);
        assert!(!fun.fact_holds(Id(2), Id(1), Fact::NONZERO));
    }
}
