use std::alloc::Allocator;

use crate::ir::Scratch;
use purple_garden_ir::{self as ir, BinOp};

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

    #[cfg(feature = "trace")]
    let name = fun.name;
    let ir::Func {
        blocks,
        params_pool,
        facts,
        ..
    } = fun;
    for block in blocks.iter().filter(|b| !b.tombstone) {
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
        for (i, _) in params_pool
            .get(args.0)
            .iter()
            .enumerate()
            .filter(|&(_, &arg)| arg == lhs)
        {
            purple_garden_shared::trace!(
                "[opt::ir::facts] {name}: param {i} of b{} is nonzero",
                target.0
            );
            facts.set_nonzero(target, i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::facts;
    use crate::ir::Scratch;
    use purple_garden_ir::{self as ir, BinOp, Block, Id, Terminator, TypeId, ptype::Type};

    /// `fn f(%v0 %v1)` with blocks `b0..b2` ending in `terms`, every block taking `%v0, %v1`.
    fn run(terms: [fn(ir::ParamsId) -> Terminator; 3]) -> ir::Func<'static> {
        let int = |id| TypeId {
            id: Id(id),
            ty: Type::Int,
        };
        let mut fun = ir::Func::new("f", Id(1), vec![int(0), int(1)], Some(Type::Int));
        let params = fun.intern_params([Id(0), Id(1)]);
        fun.blocks = terms
            .iter()
            .enumerate()
            .map(|(i, term)| Block {
                tombstone: false,
                id: Id(i as u32),
                params,
                instructions: vec![],
                term: Some(term(params)),
            })
            .collect();
        facts(&mut fun, &mut Scratch::default());
        fun
    }

    /// `br_imm IEq %v1, 0, b1(..), b2(..)`
    fn to_b2(params: ir::ParamsId) -> Terminator {
        branch(params, Id(2))
    }

    /// `br_imm IEq %v1, 0, b2(..), b0(..)`
    fn to_b0(params: ir::ParamsId) -> Terminator {
        branch(params, Id(0))
    }

    fn branch(params: ir::ParamsId, no: Id) -> Terminator {
        Terminator::BranchCmpImm {
            op: BinOp::IEq,
            lhs: Id(1),
            imm: 0,
            yes: (Id(if no == Id(2) { 1 } else { 2 }), params),
            no: (no, params),
            span: 0,
        }
    }

    fn jmp_b1(params: ir::ParamsId) -> Terminator {
        Terminator::Jump {
            id: Id(1),
            params,
            span: 0,
        }
    }

    fn jmp_b2(params: ir::ParamsId) -> Terminator {
        Terminator::Jump {
            id: Id(2),
            params,
            span: 0,
        }
    }

    fn ret(_: ir::ParamsId) -> Terminator {
        Terminator::Return {
            value: Some(Id(0)),
            span: 0,
        }
    }

    /// gcd's shape: below `b == 0`'s no edge `b` is non-zero, nowhere else.
    #[test]
    fn a_branch_proves_its_no_block_nonzero() {
        let fun = run([to_b2, ret, ret]);
        assert!(fun.nonzero(Id(2), Id(1)));
        assert!(!fun.nonzero(Id(1), Id(1)));
        assert!(!fun.nonzero(Id(0), Id(1)));
        assert!(!fun.nonzero(Id(2), Id(0)));
    }

    /// `b1` also jumps to `b2`, with `b` zero.
    #[test]
    fn a_block_with_other_predecessors_proves_nothing() {
        let fun = run([to_b2, jmp_b2, ret]);
        assert!(!fun.nonzero(Id(2), Id(1)));
    }

    /// The entry is also where the function starts, with any `b`.
    #[test]
    fn a_branch_into_the_entry_proves_nothing() {
        let fun = run([jmp_b1, to_b0, ret]);
        assert!(!fun.nonzero(Id(0), Id(1)));
    }

    /// The pass starts over, facts from an earlier run don't survive.
    #[test]
    fn clears_earlier_facts() {
        let mut fun = run([to_b2, jmp_b2, ret]);
        fun.facts.set_nonzero(Id(2), 1);
        facts(&mut fun, &mut Scratch::default());
        assert!(!fun.nonzero(Id(2), Id(1)));
    }
}
