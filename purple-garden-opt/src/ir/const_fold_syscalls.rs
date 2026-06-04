use crate::ir::Scratch;
use purple_garden_ir::{self as ir, Instr, TypeId, constant::Const};

/// Constant-fold pure syscalls whose arguments are all compile-time constants.
///
/// This pass is intentionally scaffolded around the eligibility analysis first:
/// `Instr::Sys` now carries the resolved function definition, so all data needed
/// to decide whether a syscall *can* fold is local to the IR.
pub fn const_fold_syscalls<'fold, 's>(
    fun: &'fold mut ir::Func<'s>,
    scratch: &'fold mut Scratch<'s>,
) {
    for bi in 0..fun.blocks.len() {
        if fun.blocks[bi].tombstone {
            continue;
        }

        scratch.reset();

        for ii in 0..fun.blocks[bi].instructions.len() {
            let (previous, current) = fun.blocks[bi].instructions.split_at_mut(ii);
            let instr = &mut current[0];

            if let Instr::LoadConst {
                dst: TypeId { id, .. },
                ..
            } = instr
            {
                scratch.record_const(*id, bi as u32, ii as u32);
            }

            let candidate = syscall_fold_candidate(instr, scratch, previous);
            let Some(candidate) = candidate else {
                continue;
            };

            let _ = (
                candidate.dst.id,
                candidate.fun.ptr,
                candidate.args.len(),
                candidate.span,
            );
            // TODO: invoke the macro-generated const-eval wrapper for
            // `candidate.fun`, then replace this instruction with
            // `Instr::LoadConst` carrying the returned `Const`.
        }
    }
}

#[derive(Debug)]
struct SyscallFoldCandidate<'consts, 'ir> {
    dst: TypeId<'ir>,
    fun: &'ir ir::Fn<'ir>,
    args: Vec<&'consts Const<'ir>>,
    span: u32,
}

fn syscall_fold_candidate<'consts, 'ir>(
    instr: &Instr<'ir>,
    scratch: &Scratch<'ir>,
    previous: &'consts [Instr<'ir>],
) -> Option<SyscallFoldCandidate<'consts, 'ir>> {
    let Instr::Sys {
        dst,
        fun,
        args,
        span,
        ..
    } = instr
    else {
        return None;
    };

    if !fun.pure {
        return None;
    }

    let args = args
        .iter()
        .map(|arg| {
            scratch
                .const_def(*arg)
                .and_then(|def| const_value(previous, def))
        })
        .collect::<Option<Vec<_>>>()?;

    Some(SyscallFoldCandidate {
        dst: dst.clone(),
        fun,
        args,
        span: *span,
    })
}

fn const_value<'instr, 'ir>(
    instructions: &'instr [Instr<'ir>],
    def: crate::ir::ConstDef,
) -> Option<&'instr Const<'ir>> {
    let Instr::LoadConst { value, .. } = instructions.get(def.instr as usize)? else {
        return None;
    };
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::syscall_fold_candidate;
    use crate::ir::Scratch;
    use purple_garden_ir::{
        Block, EMPTY_PARAMS, Func, Id, Instr, TypeId, constant::Const, ptype::Type,
    };
    use purple_garden_shared::BuiltinFn;
    use std::ffi::c_void;

    unsafe extern "C" fn test_syscall(_: *mut c_void) {}

    static PURE_FN: purple_garden_ir::Fn<'static> = purple_garden_ir::Fn {
        name: "pure",
        doc: "",
        ptr: test_syscall as BuiltinFn,
        pure: true,
        arg_names: &["arg"],
        args: &[Type::Int],
        ret: Type::Int,
    };

    static IMPURE_FN: purple_garden_ir::Fn<'static> = purple_garden_ir::Fn {
        name: "impure",
        doc: "",
        ptr: test_syscall as BuiltinFn,
        pure: false,
        arg_names: &["arg"],
        args: &[Type::Int],
        ret: Type::Int,
    };

    #[test]
    fn candidate_requires_pure_syscall_with_const_args() {
        let mut scratch = Scratch::default();
        scratch.record_const(Id(0), 0, 0);
        let previous = vec![Instr::LoadConst {
            dst: TypeId {
                id: Id(0),
                ty: Type::Int,
            },
            value: Const::Int(7),
            span: 0,
        }];

        let instr = Instr::Sys {
            dst: TypeId {
                id: Id(1),
                ty: Type::Int,
            },
            path: "testing",
            fun: &PURE_FN,
            args: vec![Id(0)],
            span: 12,
        };

        let candidate =
            syscall_fold_candidate(&instr, &scratch, &previous).expect("fold candidate");
        assert_eq!(candidate.args, vec![&Const::Int(7)]);
        assert_eq!(candidate.dst.id, Id(1));
        assert_eq!(candidate.fun.name, "pure");
        assert_eq!(candidate.span, 12);
    }

    #[test]
    fn candidate_rejects_impure_or_non_const_args() {
        let scratch = Scratch::default();

        let impure = Instr::Sys {
            dst: TypeId {
                id: Id(1),
                ty: Type::Int,
            },
            path: "testing",
            fun: &IMPURE_FN,
            args: vec![],
            span: 0,
        };
        assert!(syscall_fold_candidate(&impure, &scratch, &[]).is_none());

        let non_const = Instr::Sys {
            dst: TypeId {
                id: Id(1),
                ty: Type::Int,
            },
            path: "testing",
            fun: &PURE_FN,
            args: vec![Id(0)],
            span: 0,
        };
        assert!(syscall_fold_candidate(&non_const, &scratch, &[]).is_none());
    }

    #[test]
    fn scaffold_pass_keeps_ir_unchanged_until_execution_is_implemented() {
        let mut fun = Func::new("entry", Id(0), Vec::new(), Some(Type::Int));
        let block = Id(0);
        fun.blocks.push(Block {
            tombstone: false,
            id: block,
            instructions: Vec::new(),
            params: EMPTY_PARAMS,
            term: None,
        });
        fun.blocks[block.0 as usize]
            .instructions
            .push(Instr::LoadConst {
                dst: TypeId {
                    id: Id(0),
                    ty: Type::Int,
                },
                value: Const::Int(7),
                span: 0,
            });
        fun.blocks[block.0 as usize].instructions.push(Instr::Sys {
            dst: TypeId {
                id: Id(1),
                ty: Type::Int,
            },
            path: "testing",
            fun: &PURE_FN,
            args: vec![Id(0)],
            span: 0,
        });

        super::const_fold_syscalls(&mut fun, &mut Scratch::default());

        assert!(matches!(
            fun.blocks[block.0 as usize].instructions[1],
            Instr::Sys { .. }
        ));
    }
}
