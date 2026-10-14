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

    for fun in ir {
        // so all other blocks.last() are valid
        if fun.blocks.is_empty() {
            continue;
        }

        // Order: first, so const_fold folds the constants it inserts and tailcall never sees the
        // folded calls.
        //
        // Since callees are defined before use, so their entries are already recorded
        ir::const_fold_const_calls(fun, &function_consts);

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
