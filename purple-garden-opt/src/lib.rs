use std::alloc::Allocator;

use purple_garden_ir::Const;
use purple_garden_runtime::op::Op;

/// ir based optimisations
mod ir;

/// bytecode based optimisations, mainly peephole
mod bc;

/// whole-program dead function elimination, only for programs run from their entry
pub use ir::dce_functions;

pub fn ir<'f, F: Allocator, S: Allocator>(ir: &mut [purple_garden_ir::Func<'f, F>], scratch: &S) {
    let mut scratch = ir::Scratch::new_in(scratch);
    // must only be indexed with [ir::Func::id]
    let mut function_consts = Vec::with_capacity_in(ir.len(), scratch.alloc());
    // since its of size ir.len
    function_consts.resize(ir.len(), None::<Const<'f>>);
    // the slice index of each function, indexed with [ir::Func::id]
    let mut index_of = Vec::with_capacity_in(ir.len(), scratch.alloc());
    index_of.resize(ir.len(), usize::MAX);
    for (i, fun) in ir.iter().enumerate() {
        index_of[fun.id.0 as usize] = i;
    }
    // indexed with [ir::Func::id], set once a function's pipeline is done
    let mut inlinable = Vec::with_capacity_in(ir.len(), scratch.alloc());
    inlinable.resize(ir.len(), false);

    for i in 0..ir.len() {
        // callees come before their callers, `done` holds the functions already optimised
        let (done, rest) = ir.split_at_mut(i);
        let fun = &mut rest[0];
        // so all other blocks.last() are valid
        if fun.blocks.is_empty() {
            continue;
        }

        // Order: first, so const_fold folds the constants it inserts and tailcall never sees the
        // folded calls.
        //
        // Since callees are defined before use, so their entries are already recorded
        ir::const_fold_const_calls(fun, &function_consts);

        // Order: after const_fold_const_calls, which already replaced calls to constant functions,
        // and before const_fold, so it folds through the inlined bodies
        ir::inline(
            fun,
            |id| {
                let i = id.0 as usize;
                done.get(*index_of.get(i)?).filter(|_| inlinable[i])
            },
            &mut scratch,
        );

        // Order: we constant fold before imm_folding, otherwise we have fragmented half optimised
        // constant operations, we could have fully merged together at compiletime
        ir::const_fold(fun, &mut scratch);
        ir::const_fold_syscalls(fun, &mut scratch);

        // Order: after const_fold, for its constants, and before imm_fold, which would turn `x * 2`
        // into an immediate multiply
        ir::arith_simplify(fun, &mut scratch);
        ir::imm_fold(fun, &mut scratch);
        ir::branch_cmp(fun, &mut scratch);
        // Order: after branch_cmp, it folds the BranchCmp/BranchCmpImm chains
        // branch_cmp produces.
        ir::switch_fold(fun, &mut scratch);

        // Order: before indirect_jump, which would otherwise thread a jump to a
        // ret-only join into the branch and leave the join alive. Before
        // tailcall, so a Call-then-Jump-to-Ret-join pattern becomes a direct
        // Return that tailcall then picks up as Pattern A.
        ir::ret_inline(fun, &mut scratch);
        // Order: after ret_inline, which turns the arms' jumps to the join into returns.
        ir::switch_lookup(fun, &mut scratch);
        ir::indirect_jump(fun);
        ir::tailcall(fun);
        ir::addrof_fold(fun, &mut scratch);
        ir::load_store_fold(fun);

        // Order: dead code elimination always last, for the backends having less work (and the reg
        // allocator)
        ir::dce(fun, &mut scratch);

        // Order: after the whole pipeline of a function, check if the return value of the function
        // it processed is constant
        function_consts[fun.id.0 as usize] = ir::is_const_call(fun);
        inlinable[fun.id.0 as usize] = ir::inlinable(fun);
    }
}

const WINDOW_SIZE: usize = 2;

/// Peephole optimisations
///
/// Performed in-place, leaving NOP behind if instructions were replaced / removed to
/// keep JMP targets stable and enabling a single pass
///
/// See:
/// - [Peephole optimization - wikipedia](https://en.wikipedia.org/wiki/Peephole_optimization)
/// - [W. M. McKeeman "Peephole Optimization"](https://dl.acm.org/doi/epdf/10.1145/364995.365000)
pub fn bc(bc: &mut [Op]) {
    if bc.is_empty() {
        return;
    }

    for i in 0..bc.len() {
        let end = (i + WINDOW_SIZE).min(bc.len());
        let window = &mut bc[i..end];
        bc::self_move(window);
        if window.len() == WINDOW_SIZE {
            bc::mov_merge(window);
            bc::jmp_next(i, window);
        }
    }
}

#[cfg(test)]
mod tests {
    use purple_garden_ir::{
        self as ir, BinOp, Block, Const, Id, Instr, Terminator, TypeId, ptype::Type,
    };

    fn int(id: u32) -> TypeId<'static> {
        TypeId {
            id: Id(id),
            ty: Type::Int,
        }
    }

    /// `fn f{id}(%v0:Int) -> Int` with a single block of `instructions` returning `ret`.
    fn func(id: u32, instructions: Vec<Instr<'static>>, ret: u32) -> ir::Func<'static> {
        let mut fun = ir::Func::new("f", Id(id), vec![int(0)], Some(Type::Int));
        let params = fun.intern_params([Id(0)]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions,
            term: Some(Terminator::Return {
                value: Some(Id(ret)),
                span: 0,
            }),
        }];
        fun
    }

    /// `fn f{id}(%v0) { ret f{callee}(%v0) + %v0 }`, the add keeps the call from being a tail call.
    fn caller(id: u32, callee: u32) -> ir::Func<'static> {
        let mut fun = func(id, vec![], 2);
        let args = fun.intern_params([Id(0)]);
        fun.blocks[0].instructions = vec![
            Instr::Call {
                dst: int(1),
                func: Id(callee),
                args,
                span: 0,
            },
            Instr::Bin {
                op: BinOp::IAdd,
                dst: int(2),
                lhs: Id(1),
                rhs: Id(0),
                span: 0,
            },
        ];
        fun
    }

    /// `%v0 + 1 + 1 + ...` with `n` ones, each loaded separately, `n` const folds to one add.
    fn add_ones(id: u32, n: u32) -> ir::Func<'static> {
        let mut instructions = vec![];
        let mut acc = 0;
        for i in 0..n {
            let one = 1 + 2 * i;
            instructions.push(Instr::LoadConst {
                dst: int(one),
                value: Const::Int(1),
                span: 0,
            });
            instructions.push(Instr::Bin {
                op: BinOp::IAdd,
                dst: int(one + 1),
                lhs: Id(acc),
                rhs: Id(one),
                span: 0,
            });
            acc = one + 1;
        }
        func(id, instructions, acc)
    }

    fn calls(fun: &ir::Func<'static>) -> usize {
        fun.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .filter(|i| matches!(i, Instr::Call { .. }))
            .count()
    }

    fn optimise(mut funcs: Vec<ir::Func<'static>>) -> Vec<ir::Func<'static>> {
        super::ir(&mut funcs, &std::alloc::Global);
        funcs
    }

    #[test]
    fn inlines_callees_defined_before_their_caller() {
        let funcs = optimise(vec![add_ones(1, 1), caller(0, 1)]);
        assert_eq!(calls(&funcs[1]), 0, "{}", funcs[1]);
    }

    /// A callee after its caller isn't optimised yet when the caller is, so it isn't resolved.
    #[test]
    fn leaves_callees_defined_after_their_caller_alone() {
        let funcs = optimise(vec![caller(0, 1), add_ones(1, 1)]);
        assert_eq!(calls(&funcs[0]), 1, "{}", funcs[0]);
    }

    /// Unoptimised the callee is over the limit, its own pipeline folds the adds into one, and
    /// inlinability is decided on that.
    #[test]
    fn decides_inlinability_on_the_optimised_callee() {
        let callee = add_ones(1, crate::ir::MAX_INSTRUCTIONS as u32);
        assert!(!crate::ir::inlinable(&callee));
        let funcs = optimise(vec![callee, caller(0, 1)]);
        assert_eq!(calls(&funcs[1]), 0, "{}", funcs[1]);
    }
}
