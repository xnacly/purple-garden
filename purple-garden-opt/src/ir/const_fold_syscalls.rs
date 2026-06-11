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

            let Some(value) = (candidate.eval)(&candidate.args) else {
                continue;
            };

            purple_garden_shared::trace!(
                "[opt::ir::const_fold_syscalls] folded syscall into constant {:?}",
                value
            );

            let dst_id = candidate.dst.id;
            *instr = Instr::LoadConst {
                dst: candidate.dst,
                value,
                span: candidate.span,
            };
            scratch.record_const(dst_id, bi as u32, ii as u32);
        }
    }
}

#[derive(Debug)]
struct SyscallFoldCandidate<'ir> {
    dst: TypeId<'ir>,
    eval: ir::ConstEvalFn,
    args: Vec<Const<'ir>>,
    span: u32,
}

fn syscall_fold_candidate<'ir>(
    instr: &Instr<'ir>,
    scratch: &Scratch<'ir>,
    previous: &[Instr<'ir>],
) -> Option<SyscallFoldCandidate<'ir>> {
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

    let eval = fun.eval?;

    let args = args
        .iter()
        .map(|arg| {
            scratch
                .const_def(*arg)
                .and_then(|def| const_value(previous, def).cloned())
        })
        .collect::<Option<Vec<_>>>()?;

    Some(SyscallFoldCandidate {
        dst: dst.clone(),
        eval,
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
    use crate::ir::dce;
    use purple_garden_ir::{
        Block, EMPTY_PARAMS, Func, Id, Instr, TypeId, constant::Const, ptype::Type,
    };
    use purple_garden_shared::BuiltinFn;
    use std::ffi::c_void;

    unsafe extern "C" fn test_syscall(_: *mut c_void) {}

    fn double_int<'args, 'c>(args: &'args [Const<'c>]) -> Option<Const<'c>> {
        let Const::Int(value) = args.first()? else {
            return None;
        };
        Some(Const::Int(value * 2))
    }

    static PURE_FN: purple_garden_ir::Fn<'static> = purple_garden_ir::Fn {
        name: "pure",
        doc: "",
        ptr: test_syscall as BuiltinFn,
        pure: true,
        eval: Some(double_int),
        arg_names: &["arg"],
        args: &[Type::Int],
        ret: Type::Int,
    };

    static IMPURE_FN: purple_garden_ir::Fn<'static> = purple_garden_ir::Fn {
        name: "impure",
        doc: "",
        ptr: test_syscall as BuiltinFn,
        pure: false,
        eval: None,
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
        assert_eq!(candidate.args, vec![Const::Int(7)]);
        assert_eq!(candidate.dst.id, Id(1));
        assert_eq!((candidate.eval)(&candidate.args), Some(Const::Int(14)));
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
    fn impure_syscalls_stay_in_place() {
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
            fun: &IMPURE_FN,
            args: vec![Id(0)],
            span: 0,
        });

        super::const_fold_syscalls(&mut fun, &mut Scratch::default());

        assert!(matches!(
            fun.blocks[block.0 as usize].instructions[1],
            Instr::Sys { .. }
        ));
    }

    #[test]
    fn folds_pure_syscalls_with_const_args() {
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
        fun.blocks[block.0 as usize].term = Some(purple_garden_ir::Terminator::Return {
            value: Some(Id(1)),
            span: 0,
        });

        super::const_fold_syscalls(&mut fun, &mut Scratch::default());

        assert!(matches!(
            fun.blocks[block.0 as usize].instructions[1],
            Instr::LoadConst {
                value: Const::Int(14),
                ..
            }
        ));
    }

    #[test]
    fn dce_removes_dead_const_inputs_after_syscall_fold() {
        fn strlen_eval<'args, 'c>(args: &'args [Const<'c>]) -> Option<Const<'c>> {
            let Const::Str(s) = args.first()? else {
                return None;
            };
            Some(Const::Int(s.len() as i64))
        }

        static LEN_FN: purple_garden_ir::Fn<'static> = purple_garden_ir::Fn {
            name: "len",
            doc: "",
            ptr: test_syscall as BuiltinFn,
            pure: true,
            eval: Some(strlen_eval),
            arg_names: &["s"],
            args: &[Type::Str],
            ret: Type::Int,
        };

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
                    ty: Type::Str,
                },
                value: Const::from("hello world"),
                span: 0,
            });
        fun.blocks[block.0 as usize].instructions.push(Instr::Sys {
            dst: TypeId {
                id: Id(1),
                ty: Type::Int,
            },
            path: "testing",
            fun: &LEN_FN,
            args: vec![Id(0)],
            span: 0,
        });
        fun.blocks[block.0 as usize].term = Some(purple_garden_ir::Terminator::Return {
            value: Some(Id(1)),
            span: 0,
        });

        let mut scratch = Scratch::default();
        super::const_fold_syscalls(&mut fun, &mut scratch);
        dce(&mut fun, &mut scratch);

        assert!(matches!(
            fun.blocks[block.0 as usize].instructions[0],
            Instr::Noop
        ));
        assert!(matches!(
            fun.blocks[block.0 as usize].instructions[1],
            Instr::LoadConst {
                value: Const::Int(11),
                ..
            }
        ));
    }

    #[test]
    fn folds_cascading_pure_syscalls_in_one_pass() {
        fn repeat_eval<'args, 'c>(args: &'args [Const<'c>]) -> Option<Const<'c>> {
            let [Const::Str(s), Const::Int(n)] = args else {
                return None;
            };
            Some(Const::from(s.repeat(*n as usize)))
        }

        static REPEAT_FN: purple_garden_ir::Fn<'static> = purple_garden_ir::Fn {
            name: "repeat",
            doc: "",
            ptr: test_syscall as BuiltinFn,
            pure: true,
            eval: Some(repeat_eval),
            arg_names: &["s", "n"],
            args: &[Type::Str, Type::Int],
            ret: Type::Str,
        };

        let mut fun = Func::new("entry", Id(0), Vec::new(), Some(Type::Str));
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
                    ty: Type::Str,
                },
                value: Const::from("a"),
                span: 0,
            });
        fun.blocks[block.0 as usize]
            .instructions
            .push(Instr::LoadConst {
                dst: TypeId {
                    id: Id(1),
                    ty: Type::Int,
                },
                value: Const::Int(2),
                span: 0,
            });
        fun.blocks[block.0 as usize]
            .instructions
            .push(Instr::LoadConst {
                dst: TypeId {
                    id: Id(2),
                    ty: Type::Int,
                },
                value: Const::Int(3),
                span: 0,
            });
        fun.blocks[block.0 as usize].instructions.push(Instr::Sys {
            dst: TypeId {
                id: Id(3),
                ty: Type::Str,
            },
            path: "testing",
            fun: &REPEAT_FN,
            args: vec![Id(0), Id(1)],
            span: 0,
        });
        fun.blocks[block.0 as usize].instructions.push(Instr::Sys {
            dst: TypeId {
                id: Id(4),
                ty: Type::Str,
            },
            path: "testing",
            fun: &REPEAT_FN,
            args: vec![Id(3), Id(2)],
            span: 0,
        });
        fun.blocks[block.0 as usize].term = Some(purple_garden_ir::Terminator::Return {
            value: Some(Id(4)),
            span: 0,
        });

        super::const_fold_syscalls(&mut fun, &mut Scratch::default());

        assert!(matches!(
            fun.blocks[block.0 as usize].instructions[3],
            Instr::LoadConst {
                value: Const::Str(ref s),
                ..
            } if s == "aa"
        ));
        assert!(matches!(
            fun.blocks[block.0 as usize].instructions[4],
            Instr::LoadConst {
                value: Const::Str(ref s),
                ..
            } if s == "aaaaaa"
        ));
    }
}
