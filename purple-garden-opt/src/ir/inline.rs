use std::alloc::Allocator;

use crate::ir::Scratch;
use purple_garden_ir::{self as ir, Block, Id, Instr, Terminator};

/// Callees with more instructions stay calls, inlining them grows the caller and its register
/// pressure for little gain.
pub const MAX_INSTRUCTIONS: usize = 8;

/// Replaces calls to inline heuristic satisifing functions with their bodies
///
/// Heuristics are:
///
/// - must consist of a single live block, ending in `ret %vX`
/// - no `Call` or `Tail`, ruling out recursion and limits possible growth
/// - must not include `Sys` or `Lookup` to omit rebuilding of intern tables in the compiler
/// - at most [`MAX_INSTRUCTIONS`] instructions, to reduce register pressure
///
/// `callee` resolves a function id to its body, `None` for functions not optimised yet, the caller
/// itself included.
///
/// ```text
/// // is_even
/// fn f1(%v0:Int) -> Bool {
///     b0(%v0):
///         %v2:Int = IAnd %v0, 1
///         %v4:Bool = IEq %v2, 0
///         ret %v4
/// }
///
/// // collatz_steps
///     %v4:Bool = Call f1(%v0)
///     br %v4, b3(%v0, %v1), b4(%v0, %v1)
/// ```
///
/// becomes
///
/// ```text
///     %v17:Int = IAnd %v0, 1
///     %v18:Bool = IEq %v17, 0
///     br %v18, b3(%v0, %v1), b4(%v0, %v1)
/// ```
pub fn inline<'c, 'f: 'c, F: Allocator + 'c, S: Allocator>(
    fun: &mut ir::Func<'f, F>,
    callee: impl Fn(Id) -> Option<&'c ir::Func<'f, F>>,
    scratch: &mut Scratch<'_, S>,
) {
    scratch.reset();
    let mut next = next_id(fun);
    let mut args = Vec::new_in(scratch.alloc());
    let mut body = Vec::new_in(scratch.alloc());

    for bi in 0..fun.blocks.len() {
        if fun.blocks[bi].tombstone {
            continue;
        }
        let mut ii = 0;
        while ii < fun.blocks[bi].instructions.len() {
            let Instr::Call {
                dst,
                func,
                args: call_args,
                ..
            } = &fun.blocks[bi].instructions[ii]
            else {
                ii += 1;
                continue;
            };
            let Some((target, block, ret)) = callee(*func).and_then(inlinable) else {
                ii += 1;
                continue;
            };
            let dst = dst.id;
            args.clear();
            args.extend_from_slice(fun.params(*call_args));

            // a callee param is the call's arg, every other callee id moves past the caller's
            let params = target.params(block.params);
            let base = next;
            let rename = |id: &mut Id| {
                *id = match params.iter().position(|p| p == id) {
                    Some(i) => args[i],
                    None => Id(base + id.0),
                };
            };
            body.clear();
            body.extend(
                block
                    .instructions
                    .iter()
                    .filter(|instr| !matches!(instr, Instr::Noop))
                    .map(|instr| {
                        let mut instr = instr.clone();
                        instr.for_each_id_mut(rename);
                        instr
                    }),
            );
            let mut ret = ret;
            rename(&mut ret);
            next = base + next_id(target);

            purple_garden_shared::trace!(
                "[opt::ir::inline] inlined f{} into {} as %v{}",
                func.0,
                fun.name,
                ret.0
            );
            scratch.alias(dst, ret);
            // the body takes the call's place: pushed to the end, then rotated in after it
            let instructions = &mut fun.blocks[bi].instructions;
            instructions[ii] = Instr::Noop;
            for instr in &mut body {
                instructions.push(std::mem::replace(instr, Instr::Noop));
            }
            instructions[ii + 1..].rotate_right(body.len());
            ii += 1 + body.len();
        }
    }

    if !scratch.remap().is_empty() {
        fun.replace_uses(scratch.remap());
    }
}

/// `callee`, its block and the value it returns if it satisfies the heuristics of [`inline`].
fn inlinable<'c, 'f, F: Allocator>(
    callee: &'c ir::Func<'f, F>,
) -> Option<(&'c ir::Func<'f, F>, &'c Block<'f, F>, Id)> {
    let mut live = callee.blocks.iter().filter(|b| !b.tombstone);
    let (Some(block), None) = (live.next(), live.next()) else {
        return None;
    };
    let Some(Terminator::Return {
        value: Some(ret), ..
    }) = block.term
    else {
        return None;
    };
    let mut body = block
        .instructions
        .iter()
        .filter(|instr| !matches!(instr, Instr::Noop));
    if body.clone().count() > MAX_INSTRUCTIONS
        || body.any(|instr| {
            matches!(
                instr,
                Instr::Call { .. } | Instr::Sys { .. } | Instr::Lookup { .. }
            )
        })
    {
        return None;
    }
    Some((callee, block, ret))
}

/// One past the highest id `fun` takes as a param or defines.
fn next_id<F: Allocator>(fun: &ir::Func<'_, F>) -> u32 {
    let params = fun.params.iter().map(|p| p.id);
    let block_params = fun
        .blocks
        .iter()
        .flat_map(|b| fun.params(b.params))
        .copied();
    let defs = fun
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter_map(ir::Func::def_of);
    params
        .chain(block_params)
        .chain(defs)
        .map(|id| id.0 + 1)
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{MAX_INSTRUCTIONS, inline};
    use crate::ir::Scratch;
    use purple_garden_ir::{
        self as ir, BinOp, Block, Id, Instr, ParamsId, Terminator, TypeId, constant::Const,
        ptype::Type,
    };

    fn int(id: u32) -> TypeId<'static> {
        TypeId {
            id: Id(id),
            ty: Type::Int,
        }
    }

    unsafe extern "C" fn no_wrapper(_: *mut std::ffi::c_void) {}

    static ID: ir::Fn<'static> = ir::Fn {
        name: "id",
        doc: "",
        ptr: no_wrapper,
        pure: false,
        eval: None,
        arg_names: &["a"],
        args: &[Type::Int],
        ret: Type::Int,
        specialises: None,
        with_slots: false,
        intrinsic: None,
        native: None,
    };

    /// `fn f{id}(%v0:Int .. %v{params-1}:Int) -> Int` without blocks, and its param list.
    fn func(id: u32, params: u32) -> (ir::Func<'static>, ParamsId) {
        let mut fun = ir::Func::new("f", Id(id), (0..params).map(int).collect(), Some(Type::Int));
        let ids = fun.intern_params((0..params).map(Id));
        (fun, ids)
    }

    fn block(
        id: u32,
        params: ParamsId,
        instructions: Vec<Instr<'static>>,
        term: Terminator,
    ) -> Block<'static> {
        Block {
            tombstone: false,
            id: Id(id),
            params,
            instructions,
            term: Some(term),
        }
    }

    fn ret(id: u32) -> Terminator {
        Terminator::Return {
            value: Some(Id(id)),
            span: 0,
        }
    }

    /// A single block callee `f{id}` with `params` params, returning `value`.
    fn callee(id: u32, params: u32, body: Vec<Instr<'static>>, value: u32) -> ir::Func<'static> {
        let (mut fun, ids) = func(id, params);
        fun.blocks = vec![block(0, ids, body, ret(value))];
        fun
    }

    fn call(fun: &mut ir::Func<'static>, dst: u32, func: u32, args: &[u32]) -> Instr<'static> {
        Instr::Call {
            dst: int(dst),
            func: Id(func),
            args: fun.intern_params(args.iter().copied().map(Id)),
            span: 0,
        }
    }

    fn bin(op: BinOp, dst: u32, lhs: u32, rhs: u32) -> Instr<'static> {
        Instr::Bin {
            op,
            dst: int(dst),
            lhs: Id(lhs),
            rhs: Id(rhs),
            span: 0,
        }
    }

    fn load(dst: u32, value: i64) -> Instr<'static> {
        Instr::LoadConst {
            dst: int(dst),
            value: Const::Int(value),
            span: 0,
        }
    }

    fn run(fun: &mut ir::Func<'static>, callees: &[ir::Func<'static>]) {
        inline(
            fun,
            |id| callees.iter().find(|f| f.id == id),
            &mut Scratch::default(),
        );
    }

    /// `fn f9(%v0) { %v1 = Call f{func}(%v0); %v2 = IAdd %v1, %v0; ret %v2 }` with `callees`
    /// inlined.
    fn inline_into_caller(func: u32, callees: &[ir::Func<'static>]) -> ir::Func<'static> {
        let (mut fun, params) = self::func(9, 1);
        let call = call(&mut fun, 1, func, &[0]);
        fun.blocks = vec![block(
            0,
            params,
            vec![call, bin(BinOp::IAdd, 2, 1, 0)],
            ret(2),
        )];
        run(&mut fun, callees);
        fun
    }

    /// Instructions of the live blocks, without the Noops a replaced call leaves.
    fn instructions<'a>(fun: &'a ir::Func<'static>) -> Vec<&'a Instr<'static>> {
        fun.blocks
            .iter()
            .filter(|b| !b.tombstone)
            .flat_map(|b| &b.instructions)
            .filter(|i| !matches!(i, Instr::Noop))
            .collect()
    }

    fn calls(fun: &ir::Func<'static>) -> usize {
        instructions(fun)
            .into_iter()
            .filter(|i| matches!(i, Instr::Call { .. }))
            .count()
    }

    /// The `Bin` defining `dst`.
    fn operands(fun: &ir::Func<'static>, dst: u32) -> (Id, Id) {
        match instructions(fun)
            .into_iter()
            .find(|i| ir::Func::def_of(i) == Some(Id(dst)))
        {
            Some(Instr::Bin { lhs, rhs, .. }) => (*lhs, *rhs),
            other => panic!("%v{dst} is no Bin but {other:?}: {fun:?}"),
        }
    }

    /// No id is defined twice and every use is a param or defined.
    fn assert_ssa(fun: &ir::Func<'static>) {
        let mut defined: Vec<_> = fun.params.iter().map(|p| p.id).collect();
        for b in fun.blocks.iter().filter(|b| !b.tombstone) {
            defined.extend_from_slice(fun.params(b.params));
        }
        defined.sort_by_key(|id| id.0);
        defined.dedup();
        for instr in instructions(fun) {
            fun.for_each_use_of_instr(instr, |id| {
                assert!(
                    defined.contains(&id),
                    "%v{} used before its definition: {fun:?}",
                    id.0
                )
            });
            if let Some(dst) = ir::Func::def_of(instr) {
                assert!(
                    !defined.contains(&dst),
                    "%v{} defined twice: {fun:?}",
                    dst.0
                );
                defined.push(dst);
            }
        }
        for b in fun.blocks.iter().filter(|b| !b.tombstone) {
            fun.for_each_use_of_term(b.term.as_ref().unwrap(), |id| {
                assert!(defined.contains(&id), "%v{} is not defined: {fun:?}", id.0)
            });
        }
    }

    /// `f1(%v0) { %v1 = 1; %v2 = IAdd %v0, %v1; ret %v2 }`
    fn add_one() -> ir::Func<'static> {
        callee(1, 1, vec![load(1, 1), bin(BinOp::IAdd, 2, 0, 1)], 2)
    }

    /// The id the inlined `IAdd` of [`add_one`] defines, checking it adds `arg` and one.
    fn inlined_add_one(fun: &ir::Func<'static>, arg: Id) -> Id {
        let Some(Instr::Bin { dst, rhs, .. }) = instructions(fun)
            .into_iter()
            .find(|i| matches!(i, Instr::Bin { op: BinOp::IAdd, lhs, .. } if *lhs == arg))
        else {
            panic!("no inlined IAdd of %v{}: {fun:?}", arg.0);
        };
        assert!(instructions(fun).into_iter().any(|i| matches!(
            i,
            Instr::LoadConst { dst, value: Const::Int(1), .. } if dst.id == *rhs
        )));
        dst.id
    }

    #[test]
    fn inlines_a_single_block_callee() {
        let fun = inline_into_caller(1, &[add_one()]);
        assert_eq!(calls(&fun), 0);
        let one_more = inlined_add_one(&fun, Id(0));
        assert_eq!(operands(&fun, 2), (one_more, Id(0)));
        assert_ssa(&fun);
    }

    /// The body takes the call's place, before what followed it.
    #[test]
    fn keeps_the_body_where_the_call_was() {
        let fun = inline_into_caller(1, &[add_one()]);
        let order: Vec<_> = instructions(&fun)
            .into_iter()
            .map(|i| match i {
                Instr::LoadConst { .. } => "const",
                Instr::Bin { dst, .. } if dst.id == Id(2) => "caller",
                Instr::Bin { .. } => "inlined",
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(order, ["const", "inlined", "caller"]);
    }

    /// `f1(%v0) { ret %v0 }`: the call's value is its argument, nothing is inlined.
    #[test]
    fn passes_a_returned_param_through() {
        let fun = inline_into_caller(1, &[callee(1, 1, vec![], 0)]);
        assert!(instructions(&fun).len() == 1);
        assert_eq!(operands(&fun, 2), (Id(0), Id(0)));
    }

    /// `f1(%v0 %v1) { ret %v0 - %v1 }` called as `f1(%v1 %v0)`: each param takes its own arg.
    #[test]
    fn maps_each_param_to_its_arg() {
        let sub = callee(1, 2, vec![bin(BinOp::ISub, 2, 0, 1)], 2);
        let (mut fun, params) = func(9, 2);
        let call = call(&mut fun, 2, 1, &[1, 0]);
        fun.blocks = vec![block(0, params, vec![call], ret(2))];
        run(&mut fun, &[sub]);

        let Some(Instr::Bin {
            op: BinOp::ISub,
            lhs,
            rhs,
            ..
        }) = instructions(&fun).first().copied()
        else {
            panic!("no inlined ISub: {fun:?}");
        };
        assert_eq!((*lhs, *rhs), (Id(1), Id(0)));
        assert_ssa(&fun);
    }

    /// The caller already defines ids above the callee's, the inlined ones go past them.
    #[test]
    fn inlined_ids_go_past_the_callers() {
        let (mut fun, params) = func(9, 1);
        let call = call(&mut fun, 1, 1, &[50]);
        fun.blocks = vec![block(
            0,
            params,
            vec![load(50, 7), call, bin(BinOp::IAdd, 2, 1, 50)],
            ret(2),
        )];
        run(&mut fun, &[add_one()]);
        let one_more = inlined_add_one(&fun, Id(50));
        assert!(one_more.0 > 50, "{fun:?}");
        assert_ssa(&fun);
    }

    /// `f1(f1(%v0))`: the second inlined body reads the first's result, each with its own ids.
    #[test]
    fn inlines_calls_on_inlined_results() {
        let (mut fun, params) = func(9, 1);
        let first = call(&mut fun, 1, 1, &[0]);
        let second = call(&mut fun, 2, 1, &[1]);
        fun.blocks = vec![block(0, params, vec![first, second], ret(2))];
        run(&mut fun, &[add_one()]);

        assert_eq!(calls(&fun), 0);
        let inner = inlined_add_one(&fun, Id(0));
        let outer = inlined_add_one(&fun, inner);
        let Some(Terminator::Return { value, .. }) = fun.blocks[0].term else {
            unreachable!()
        };
        assert_eq!(value, Some(outer));
        assert_ssa(&fun);
    }

    /// The call's value read by a branch, a jump's args and a return, in a block after the entry.
    #[test]
    fn rewrites_uses_in_terminators_and_later_blocks() {
        let (mut fun, params) = func(9, 1);
        let call = call(&mut fun, 1, 1, &[0]);
        let to_join = fun.intern_params([Id(1)]);
        let join_params = fun.intern_params([Id(2)]);
        let empty = fun.intern_params([]);
        fun.blocks = vec![
            block(
                0,
                params,
                vec![],
                Terminator::Jump {
                    id: Id(1),
                    params: empty,
                    span: 0,
                },
            ),
            block(
                1,
                empty,
                vec![call],
                Terminator::Branch {
                    cond: Id(1),
                    yes: (Id(2), to_join),
                    no: (Id(3), empty),
                    span: 0,
                },
            ),
            block(2, join_params, vec![], ret(2)),
            block(3, empty, vec![], ret(1)),
        ];
        run(&mut fun, &[add_one()]);

        let one_more = inlined_add_one(&fun, Id(0));
        let Some(Terminator::Branch { cond, yes, .. }) = fun.blocks[1].term else {
            unreachable!()
        };
        assert_eq!(cond, one_more);
        assert_eq!(fun.params(yes.1), [one_more]);
        assert!(matches!(
            fun.blocks[3].term,
            Some(Terminator::Return { value: Some(v), .. }) if v == one_more
        ));
        assert_ssa(&fun);
    }

    /// Two calls of one callee each get their own copy.
    #[test]
    fn inlines_every_call_of_a_callee() {
        let (mut fun, params) = func(9, 1);
        let first = call(&mut fun, 1, 1, &[0]);
        let second = call(&mut fun, 2, 1, &[0]);
        fun.blocks = vec![block(
            0,
            params,
            vec![first, second, bin(BinOp::IAdd, 3, 1, 2)],
            ret(3),
        )];
        run(&mut fun, &[add_one()]);

        assert_eq!(calls(&fun), 0);
        let (lhs, rhs) = operands(&fun, 3);
        assert_ne!(lhs, rhs);
        for id in [lhs, rhs] {
            assert!(matches!(
                instructions(&fun)
                    .into_iter()
                    .find(|i| ir::Func::def_of(i) == Some(id)),
                Some(Instr::Bin {
                    op: BinOp::IAdd,
                    lhs: Id(0),
                    ..
                })
            ));
        }
        assert_ssa(&fun);
    }

    /// An inlined assert still traps, at the callee's source location.
    #[test]
    fn keeps_asserts_and_spans() {
        let body = vec![
            Instr::BinImm {
                op: BinOp::IGt,
                dst: TypeId {
                    id: Id(1),
                    ty: Type::Bool,
                },
                lhs: Id(0),
                imm: 0,
                span: 42,
            },
            Instr::Assert {
                dst: TypeId {
                    id: Id(2),
                    ty: Type::Void,
                },
                cond: Id(1),
                span: 43,
            },
        ];
        let fun = inline_into_caller(1, &[callee(1, 1, body, 0)]);
        assert_eq!(calls(&fun), 0);
        let spans: Vec<_> = instructions(&fun)
            .into_iter()
            .filter(|i| matches!(i, Instr::BinImm { .. } | Instr::Assert { .. }))
            .map(Instr::span)
            .collect();
        assert_eq!(spans, [42, 43]);
        assert_eq!(operands(&fun, 2), (Id(0), Id(0)));
        assert_ssa(&fun);
    }

    /// Noops neither get copied nor count towards the limit, neither do tombstoned blocks.
    #[test]
    fn ignores_noops_and_tombstones_in_the_callee() {
        let mut body = vec![Instr::Noop; MAX_INSTRUCTIONS];
        body.push(load(1, 1));
        body.push(bin(BinOp::IAdd, 2, 0, 1));
        let mut target = callee(1, 1, body, 2);
        let mut dead = target.blocks[0].clone();
        dead.tombstone = true;
        target.blocks.insert(0, dead);

        let fun = inline_into_caller(1, &[target]);
        assert_eq!(calls(&fun), 0);
        inlined_add_one(&fun, Id(0));
        assert_eq!(instructions(&fun).len(), 3);
    }

    #[test]
    fn inlines_callees_at_the_limit() {
        let body = (1..=MAX_INSTRUCTIONS as u32)
            .map(|id| Instr::BinImm {
                op: BinOp::IAdd,
                dst: int(id),
                lhs: Id(id - 1),
                imm: 1,
                span: 0,
            })
            .collect();
        let fun = inline_into_caller(1, &[callee(1, 1, body, MAX_INSTRUCTIONS as u32)]);
        assert_eq!(calls(&fun), 0);
        assert_ssa(&fun);
    }

    #[test]
    fn leaves_callees_above_the_limit_alone() {
        let body = (1..=MAX_INSTRUCTIONS as u32 + 1)
            .map(|id| Instr::BinImm {
                op: BinOp::IAdd,
                dst: int(id),
                lhs: Id(id - 1),
                imm: 1,
                span: 0,
            })
            .collect();
        let target = callee(1, 1, body, MAX_INSTRUCTIONS as u32 + 1);
        assert_eq!(calls(&inline_into_caller(1, &[target])), 1);
    }

    #[test]
    fn leaves_multi_block_callees_alone() {
        let (mut target, params) = func(1, 1);
        target.blocks = vec![
            block(0, params, vec![], ret(0)),
            block(1, params, vec![], ret(0)),
        ];
        assert_eq!(calls(&inline_into_caller(1, &[target])), 1);
    }

    #[test]
    fn leaves_callees_with_calls_alone() {
        let (mut target, params) = func(1, 1);
        let call = call(&mut target, 1, 2, &[0]);
        target.blocks = vec![block(0, params, vec![call], ret(1))];
        let mut add_one = add_one();
        add_one.id = Id(2);
        assert_eq!(calls(&inline_into_caller(1, &[target, add_one])), 1);
    }

    /// Their args and tables live in the callee's pools.
    #[test]
    fn leaves_callees_with_sys_or_lookup_alone() {
        let (mut sys, params) = func(1, 1);
        let args = sys.intern_params([Id(0)]);
        sys.blocks = vec![block(
            0,
            params,
            vec![Instr::Sys {
                dst: int(1),
                path: "t",
                fun: &ID,
                args,
                span: 0,
            }],
            ret(1),
        )];

        let (mut lookup, params) = func(1, 1);
        let entries = lookup.intern_entries([(Const::Int(0), Const::Int(1))]);
        lookup.blocks = vec![block(
            0,
            params,
            vec![Instr::Lookup {
                dst: int(1),
                subject: Id(0),
                entries,
                default: Const::Int(0),
                span: 0,
            }],
            ret(1),
        )];

        for target in [sys, lookup] {
            assert_eq!(calls(&inline_into_caller(1, &[target])), 1);
        }
    }

    /// A Void `ret` has no value for the call's dst to become.
    #[test]
    fn leaves_callees_without_a_returned_value_alone() {
        let (mut target, params) = func(1, 1);
        target.blocks = vec![block(
            0,
            params,
            vec![],
            Terminator::Return {
                value: None,
                span: 0,
            },
        )];
        assert_eq!(calls(&inline_into_caller(1, &[target])), 1);
    }

    /// Tail calls end in `Tail`, not `ret`.
    #[test]
    fn leaves_tail_calling_callees_alone() {
        let (mut target, params) = func(1, 1);
        let args = target.intern_params([Id(0)]);
        target.blocks = vec![block(
            0,
            params,
            vec![],
            Terminator::Tail {
                func: Id(2),
                args,
                span: 0,
            },
        )];
        assert_eq!(calls(&inline_into_caller(1, &[target])), 1);
    }

    /// Functions not optimised yet, the caller itself included, resolve to `None`.
    #[test]
    fn leaves_unresolved_callees_alone() {
        assert_eq!(calls(&inline_into_caller(9, &[add_one()])), 1);
    }

    #[test]
    fn skips_tombstoned_caller_blocks() {
        let (mut fun, params) = func(9, 1);
        let call = call(&mut fun, 1, 1, &[0]);
        let mut dead = block(0, params, vec![call], ret(1));
        dead.tombstone = true;
        fun.blocks = vec![dead, block(1, params, vec![], ret(0))];
        run(&mut fun, &[add_one()]);
        assert!(matches!(
            fun.blocks[0].instructions[..],
            [Instr::Call { .. }]
        ));
    }
}
