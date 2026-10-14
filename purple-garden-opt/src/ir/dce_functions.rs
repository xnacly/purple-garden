use std::alloc::Allocator;

use purple_garden_ir::{Func, Instr, Terminator};

/// Dead function elimination: marks every function the entry (`f0`) can't reach as
/// [`Func::unused`], so the backends don't lower it.
///
/// Functions are defined before they are used, so callees precede their callers and the entry
/// comes last. A single backwards walk thus sees every caller of a function before the function
/// itself:
///
/// ```text
/// // unused; not lowered
/// // PI
/// fn f1() -> Double {
/// b0():
///         %v0:Double = 3.1415
///         ret %v0
/// }
/// ```
///
/// Only for programs that are run from their entry, embedders may call any function.
pub fn dce_functions<F: Allocator, S: Allocator>(ir: &mut [Func<'_, F>], scratch: &S) {
    // must only be indexed with [Func::id]
    let mut live = Vec::with_capacity_in(ir.len(), scratch);
    live.resize(ir.len(), false);
    live[0] = true;

    for fun in ir.iter_mut().rev() {
        if !live[fun.id.0 as usize] {
            purple_garden_shared::trace!("[opt::dce_functions] f{} is never called", fun.id.0);
            fun.unused = true;
            continue;
        }

        for block in fun.blocks.iter().filter(|b| !b.tombstone) {
            for instr in &block.instructions {
                if let Instr::Call { func, .. } = instr {
                    live[func.0 as usize] = true;
                }
            }
            if let Some(Terminator::Tail { func, .. }) = block.term {
                live[func.0 as usize] = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::dce_functions;
    use purple_garden_ir::{Block, EMPTY_PARAMS, Func, Id, Instr, Terminator, TypeId, ptype::Type};
    use std::alloc::Global;

    fn func(id: u32, instructions: Vec<Instr<'static>>, term: Terminator) -> Func<'static> {
        let mut fun = Func::new("f", Id(id), vec![], Some(Type::Int));
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            instructions,
            params: EMPTY_PARAMS,
            term: Some(term),
        }];
        fun
    }

    fn ret() -> Terminator {
        Terminator::Return {
            value: None,
            span: 0,
        }
    }

    fn call(callee: u32) -> Instr<'static> {
        Instr::Call {
            dst: TypeId {
                id: Id(0),
                ty: Type::Int,
            },
            func: Id(callee),
            args: EMPTY_PARAMS,
            span: 0,
        }
    }

    /// f1 is only called by f2, which nothing calls, so both are dead even though f1 has a caller.
    #[test]
    fn keeps_only_functions_reachable_from_entry() {
        let mut ir = [
            func(1, vec![], ret()),
            func(2, vec![call(1)], ret()),
            func(3, vec![], ret()),
            func(
                4,
                vec![],
                Terminator::Tail {
                    func: Id(3),
                    args: EMPTY_PARAMS,
                    span: 0,
                },
            ),
            func(0, vec![call(4)], ret()),
        ];

        dce_functions(&mut ir, &Global);

        let unused: Vec<_> = ir.iter().map(|f| (f.id.0, f.unused)).collect();
        assert_eq!(
            unused,
            [(1, true), (2, true), (3, false), (4, false), (0, false)]
        );
    }
}
