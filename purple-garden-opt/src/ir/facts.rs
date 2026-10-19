use std::alloc::Allocator;

use crate::ir::Scratch;
use purple_garden_ir::{self as ir, BinOp, Fact};

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
pub fn facts<F: Allocator, S: Allocator>(fun: &mut ir::Func<'_, F>, scratch: &mut Scratch<'_, S>) {
    fun.facts.clear();
    // entered at the start and by self tail calls too, a branch into it proves nothing
    let Some(entry) = fun.blocks.iter().find(|b| !b.tombstone).map(|b| b.id) else {
        return;
    };
    let predecessors = super::predecessor_counts(fun, scratch.alloc());

    let mut nonzero = Vec::new_in(scratch.alloc());
    for block in fun.blocks.iter().filter(|b| !b.tombstone) {
        let Some(ir::Terminator::BranchCmpImm {
            op: BinOp::IEq,
            lhs,
            imm: 0,
            no: (target, args),
            ..
        }) = block.term
        else {
            continue;
        };
        if target == entry || predecessors[target.0 as usize] != 1 {
            continue;
        }
        for (i, &arg) in fun.params(args).iter().enumerate() {
            if arg == lhs {
                nonzero.push((target, i));
            }
        }
    }

    for (block, i) in nonzero {
        purple_garden_shared::trace!(
            "[opt::ir::facts] {}: param {} of b{} is nonzero",
            fun.name,
            i,
            block.0
        );
        fun.facts.add_param(block, i, Fact::NONZERO);
    }
}

#[cfg(test)]
mod tests {
    use super::facts;
    use crate::ir::Scratch;
    use purple_garden_ir::{
        self as ir, BinOp, Block, Fact, Id, Instr, Terminator, TypeId, ptype::Type,
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
    fn a_branch_proves_its_no_block_nonzero() {
        let mut fun = gcd(false);
        facts(&mut fun, &mut Scratch::default());
        assert!(fun.fact_holds(Id(2), Id(1), Fact::NONZERO));
        assert!(!fun.fact_holds(Id(1), Id(1), Fact::NONZERO));
        assert!(!fun.fact_holds(Id(0), Id(1), Fact::NONZERO));
    }

    /// `b2` is also reached from `b1`, where `b` is zero.
    #[test]
    fn a_block_with_other_predecessors_proves_nothing() {
        let mut fun = gcd(true);
        facts(&mut fun, &mut Scratch::default());
        assert!(!fun.fact_holds(Id(2), Id(1), Fact::NONZERO));
    }

    /// `b1` loops back into the entry `b0` when `%v1` isn't zero, but the entry is also where the
    /// function starts, with any `%v1`.
    #[test]
    fn a_branch_into_the_entry_proves_nothing() {
        let mut fun = ir::Func::new("f", Id(1), vec![int(0), int(1)], Some(Type::Int));
        let params = fun.intern_params([Id(0), Id(1)]);
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params,
                instructions: vec![Instr::Bin {
                    op: BinOp::IMod,
                    dst: int(2),
                    lhs: Id(0),
                    rhs: Id(1),
                    span: 0,
                }],
                term: Some(Terminator::Jump {
                    id: Id(1),
                    params,
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params,
                instructions: vec![],
                term: Some(Terminator::BranchCmpImm {
                    op: BinOp::IEq,
                    lhs: Id(1),
                    imm: 0,
                    yes: (Id(2), params),
                    no: (Id(0), params),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(2),
                params,
                instructions: vec![],
                term: ret(0),
            },
        ];
        facts(&mut fun, &mut Scratch::default());
        assert!(!fun.fact_holds(Id(0), Id(1), Fact::NONZERO));
    }

    /// The pass starts over, facts from an earlier run don't survive.
    #[test]
    fn clears_earlier_facts() {
        let mut fun = gcd(true);
        fun.facts.add_param(Id(2), 1, Fact::NONZERO);
        facts(&mut fun, &mut Scratch::default());
        assert!(!fun.fact_holds(Id(2), Id(1), Fact::NONZERO));
    }
}
