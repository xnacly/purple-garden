use std::alloc::Allocator;

use purple_garden_ir::Const;
use purple_garden_runtime::op::Op;

/// ir based optimisations
mod ir;

/// bytecode based optimisations, mainly peephole
mod bc;

/// whole-program dead function elimination, only for programs run from their entry
pub use ir::dce_functions;

/// An IR pass of [`ir`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    ConstFoldConstCalls,
    Inline,
    ConstFold,
    ConstFoldSyscalls,
    ArithSimplify,
    ImmFold,
    BranchCmp,
    SwitchFold,
    RetInline,
    SwitchLookup,
    IndirectJump,
    Tailcall,
    AddrofFold,
    LoadStoreFold,
    Dce,
    Facts,
}

impl Pass {
    /// The passes in the order [`ir`] runs them, a pass running more than once is listed each time.
    pub const PIPELINE: [Pass; 17] = [
        Pass::ConstFoldConstCalls,
        Pass::Inline,
        Pass::LoadStoreFold,
        Pass::ConstFold,
        Pass::ConstFoldSyscalls,
        Pass::ArithSimplify,
        Pass::ImmFold,
        Pass::BranchCmp,
        Pass::SwitchFold,
        Pass::RetInline,
        Pass::SwitchLookup,
        Pass::IndirectJump,
        Pass::Tailcall,
        Pass::AddrofFold,
        Pass::LoadStoreFold,
        Pass::Dce,
        Pass::Facts,
    ];

    /// The pass' module name, as `NRFC/optimisation-passes.md` lists it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Pass::ConstFoldConstCalls => "const_fold_const_calls",
            Pass::Inline => "inline",
            Pass::ConstFold => "const_fold",
            Pass::ConstFoldSyscalls => "const_fold_syscalls",
            Pass::ArithSimplify => "arith_simplify",
            Pass::ImmFold => "imm_fold",
            Pass::BranchCmp => "branch_cmp",
            Pass::SwitchFold => "switch_fold",
            Pass::RetInline => "ret_inline",
            Pass::SwitchLookup => "switch_lookup",
            Pass::IndirectJump => "indirect_jump",
            Pass::Tailcall => "tailcall",
            Pass::AddrofFold => "addrof_fold",
            Pass::LoadStoreFold => "load_store_fold",
            Pass::Dce => "dce",
            Pass::Facts => "facts",
        }
    }
}

pub fn ir<'f, F: Allocator, S: Allocator>(ir: &mut [purple_garden_ir::Func<'f, F>], scratch: &S) {
    ir_observed(ir, scratch, |_, _| {});
}

/// [`ir`], calling `observe` with every function after each of its passes.
pub fn ir_observed<'f, F: Allocator, S: Allocator>(
    ir: &mut [purple_garden_ir::Func<'f, F>],
    scratch: &S,
    mut observe: impl FnMut(Pass, &purple_garden_ir::Func<'f, F>),
) {
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
        observe(Pass::ConstFoldConstCalls, fun);

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
        observe(Pass::Inline, fun);

        // Order: before const_fold, so a load of a stored array length or record field folds
        // through, e.g. away the bounds check of a literal array indexed by a constant
        ir::load_store_fold(fun, &mut scratch);
        observe(Pass::LoadStoreFold, fun);

        // Order: we constant fold before imm_folding, otherwise we have fragmented half optimised
        // constant operations, we could have fully merged together at compiletime
        ir::const_fold(fun, &mut scratch);
        observe(Pass::ConstFold, fun);
        ir::const_fold_syscalls(fun, &mut scratch);
        observe(Pass::ConstFoldSyscalls, fun);

        // Order: after const_fold, for its constants, and before imm_fold, which would turn `x * 2`
        // into an immediate multiply
        ir::arith_simplify(fun, &mut scratch);
        observe(Pass::ArithSimplify, fun);
        ir::imm_fold(fun, &mut scratch);
        observe(Pass::ImmFold, fun);
        ir::branch_cmp(fun, &mut scratch);
        observe(Pass::BranchCmp, fun);
        // Order: after branch_cmp, it folds the BranchCmp/BranchCmpImm chains
        // branch_cmp produces.
        ir::switch_fold(fun, &mut scratch);
        observe(Pass::SwitchFold, fun);

        // Order: before indirect_jump, which would otherwise thread a jump to a
        // ret-only join into the branch and leave the join alive. Before
        // tailcall, so a Call-then-Jump-to-Ret-join pattern becomes a direct
        // Return that tailcall then picks up as Pattern A.
        ir::ret_inline(fun, &mut scratch);
        observe(Pass::RetInline, fun);
        // Order: after ret_inline, which turns the arms' jumps to the join into returns.
        ir::switch_lookup(fun, &mut scratch);
        observe(Pass::SwitchLookup, fun);
        ir::indirect_jump(fun);
        observe(Pass::IndirectJump, fun);
        ir::tailcall(fun);
        observe(Pass::Tailcall, fun);
        ir::addrof_fold(fun, &mut scratch);
        observe(Pass::AddrofFold, fun);
        // Order: again after arith_simplify and addrof_fold, which turn computed element and field
        // addresses into the base+offset the stores used
        ir::load_store_fold(fun, &mut scratch);
        observe(Pass::LoadStoreFold, fun);

        // Order: dead code elimination always last, for the backends having less work (and the reg
        // allocator)
        ir::dce(fun, &mut scratch);
        observe(Pass::Dce, fun);

        // Order: after every rewrite, nothing may change the IR once its facts are recorded
        ir::facts(fun, &mut scratch);
        observe(Pass::Facts, fun);

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

    #[test]
    fn observes_every_pass_of_every_function_in_pipeline_order() {
        let mut funcs = vec![add_ones(1, 2), caller(0, 1)];
        let mut seen = vec![];
        super::ir_observed(&mut funcs, &std::alloc::Global, |pass, fun| {
            seen.push((fun.id, pass));
        });
        let want: Vec<_> = [Id(1), Id(0)]
            .into_iter()
            .flat_map(|id| super::Pass::PIPELINE.map(|pass| (id, pass)))
            .collect();
        assert_eq!(seen, want);
    }

    /// Each snapshot is the function right after its pass: the call is still there after
    /// `const_fold_const_calls` and inlined after `inline`.
    #[test]
    fn observes_the_ir_right_after_each_pass() {
        let mut funcs = vec![add_ones(1, 1), caller(0, 1)];
        let mut caller_calls = vec![];
        super::ir_observed(&mut funcs, &std::alloc::Global, |pass, fun| {
            if fun.id == Id(0) {
                caller_calls.push((pass, calls(fun)));
            }
        });
        assert_eq!(caller_calls[0], (super::Pass::ConstFoldConstCalls, 1));
        assert_eq!(caller_calls[1], (super::Pass::Inline, 0));
    }

    #[test]
    fn observing_does_not_change_the_result() {
        let input = || vec![add_ones(1, 3), caller(0, 1), caller(2, 0)];
        let mut observed = input();
        super::ir_observed(&mut observed, &std::alloc::Global, |_, _| {});
        let plain = optimise(input());
        assert_eq!(
            plain.iter().map(ToString::to_string).collect::<Vec<_>>(),
            observed.iter().map(ToString::to_string).collect::<Vec<_>>()
        );
    }
}
