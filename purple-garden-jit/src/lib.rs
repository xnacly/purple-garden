//! Baseline JIT backend.
//!
//! This is not an optimizing native-code compiler; but rather a dispatch remover.
//! [`Jit::compile_func`] lowers a supported IR function to native code
//! thats reading and writing the VM register file in place. The native ABI passes
//! `*mut Vm` in the first argument register, due to `Vm::r` being the first
//! field of `Vm`.
//!
//! The bytecode backend is the complete backend. Native compilation is an
//! opportunistic function based fast path: if the target-specific backend returns
//! `None`, the bytecode compiler lowers the same IR function normally. A
//! successful native function is retained as executable memory and injected into
//! the VM as a syscall entry, so the VM can dispatch native and Rust builtins
//! through the same `Op::Sys` mechanism.
//!
//! The high-level pipeline is:
//!
//! 1. compute shared IR liveness
//! 2. ask the active architecture backend to plan the function (based on support, amount of
//!    registers used, size of function and other heuristics)
//! 3. allocate physical registers from the liveness and target constraints
//! 4. emit native code
//! 5. expose the emitted page as a [`JitFn`]
//!
//! The architecture backend owns the supported IR subset. This crate should
//! therefore be read as a conservative native lowering path, not as a second
//! semantic implementation of the language.

#[cfg(not(all(
    any(target_os = "linux", target_os = "macos"),
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
compile_error!("purple-garden-jit currently supports only Linux or macOS on x86_64 or aarch64");

#[cfg(target_arch = "x86_64")]
#[path = "x86/mod.rs"]
mod arch;
#[cfg(target_arch = "aarch64")]
#[path = "aarch64/mod.rs"]
mod arch;
pub mod mem;
mod regalloc;

pub use mem::JitFn;
use purple_garden_ir as ir;

/// Reusable JIT codegen state.
#[derive(Debug, Default, Clone)]
pub struct Jit {
    code: Vec<u8>,
    liveness: Vec<(u32, u32)>,
    regalloc: regalloc::Allocator,
}

impl Jit {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Lower and encode `func`, returning `None` when unsupported.
    pub fn compile_func(&mut self, func: &ir::Func<'_>) -> Option<()> {
        self.liveness.clear();
        func.live_set_into(&mut self.liveness);
        let liveness = std::mem::take(&mut self.liveness);
        let result = self.compile_func_with_liveness(func, &liveness);
        self.liveness = liveness;
        result
    }

    /// Lower and encode `func` using precomputed liveness.
    pub fn compile_func_with_liveness(
        &mut self,
        func: &ir::Func<'_>,
        liveness: &[(u32, u32)],
    ) -> Option<()> {
        self.code.clear();
        let result = arch::compile_func(func, &mut self.code, liveness, &mut self.regalloc);
        if result.is_none() {
            self.code.clear();
        }
        result
    }

    /// The machine code for the most recent [`Jit::compile_func`].
    #[must_use]
    pub fn code(&self) -> &[u8] {
        &self.code
    }
}

#[cfg(all(
    test,
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "macos")
))]
mod tests_x86 {
    use super::Jit;
    use super::mem::ExecPage;
    use purple_garden_ir::{
        BinOp, Block, Const, EMPTY_PARAMS, Func, Id, Instr, Terminator, TypeId, ptype::Type,
    };

    /// Run native code that takes `*mut u64` (the VM register file) and return
    /// the resulting register slots.
    fn run(code: &[u8], mut regs: [u64; 3]) -> [u64; 3] {
        let page = ExecPage::new(code).expect("mmap");
        let f: unsafe extern "C" fn(*mut u64) = unsafe { std::mem::transmute(page.as_ptr()) };
        unsafe { f(regs.as_mut_ptr()) };
        regs
    }

    /// `fn identity(a) int { a }`: load the arg, store it back as the result.
    #[test]
    fn identity_returns_arg() {
        let mut func = Func::new("identity", Id(0), vec![Id(0)], Some(Type::Int));
        let params = func.intern_params(vec![Id(0)]);
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            instructions: vec![],
            params,
            term: Some(Terminator::Return {
                value: Some(Id(0)),
                span: 0,
            }),
        });

        let mut jit = Jit::new();
        jit.compile_func(&func).expect("jit function");
        assert_eq!(run(jit.code(), [42, 0xdead, 0xaffe]), [42, 0xdead, 0xaffe]);
    }

    /// `fn second(a b) int { b }`: the result is the second arg (vm.r[1]).
    #[test]
    fn returns_second_arg() {
        let mut func = Func::new("second", Id(0), vec![Id(0), Id(1)], Some(Type::Int));
        let params = func.intern_params(vec![Id(0), Id(1)]);
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            instructions: vec![],
            params,
            term: Some(Terminator::Return {
                value: Some(Id(1)),
                span: 0,
            }),
        });

        let mut jit = Jit::new();
        jit.compile_func(&func).expect("jit function");
        assert_eq!(run(jit.code(), [10, 20, 0])[0], 20);
    }

    /// `fn const_ret() int { 42 }`: LoadConst is supported, so a const-returning
    /// function compiles and yields the constant in vm.r[0].
    #[test]
    fn compiles_const_returning_function() {
        let mut func = Func::new("const_ret", Id(0), vec![], Some(Type::Int));
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            instructions: vec![Instr::LoadConst {
                dst: TypeId {
                    id: Id(0),
                    ty: Type::Int,
                },
                value: Const::Int(42),
                span: 0,
            }],
            params: EMPTY_PARAMS,
            term: Some(Terminator::Return {
                value: Some(Id(0)),
                span: 0,
            }),
        });

        let mut jit = Jit::new();
        jit.compile_func(&func).expect("jit function");
        assert_eq!(run(jit.code(), [0, 0, 0])[0], 42);
    }

    #[test]
    fn compiles_tail_recursive_factorial_loop() {
        let mut func = Func::new("factorial", Id(0), vec![Id(0), Id(1)], Some(Type::Int));
        let params = func.intern_params(vec![Id(0), Id(1)]);
        let ret_args = func.intern_params(vec![Id(1)]);
        let ret_params = func.intern_params(vec![Id(7)]);
        func.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                instructions: vec![],
                params,
                term: None,
            },
            Block {
                tombstone: false,
                id: Id(1),
                instructions: vec![Instr::BinImm {
                    op: BinOp::IEq,
                    dst: TypeId {
                        id: Id(3),
                        ty: Type::Bool,
                    },
                    lhs: Id(0),
                    imm: 0,
                    span: 0,
                }],
                params,
                term: Some(Terminator::Branch {
                    cond: Id(3),
                    yes: (Id(4), ret_args),
                    no: (Id(3), params),
                    span: 0,
                }),
            },
            Block {
                tombstone: true,
                id: Id(2),
                instructions: vec![],
                params: EMPTY_PARAMS,
                term: None,
            },
            Block {
                tombstone: false,
                id: Id(3),
                instructions: vec![
                    Instr::BinImm {
                        op: BinOp::ISub,
                        dst: TypeId {
                            id: Id(5),
                            ty: Type::Int,
                        },
                        lhs: Id(0),
                        imm: 1,
                        span: 0,
                    },
                    Instr::Bin {
                        op: BinOp::IMul,
                        dst: TypeId {
                            id: Id(6),
                            ty: Type::Int,
                        },
                        lhs: Id(0),
                        rhs: Id(1),
                        span: 0,
                    },
                ],
                params,
                term: Some(Terminator::Tail {
                    func: Id(0),
                    args: vec![Id(5), Id(6)],
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(4),
                instructions: vec![],
                params: ret_params,
                term: Some(Terminator::Return {
                    value: Some(Id(7)),
                    span: 0,
                }),
            },
        ];

        let mut jit = Jit::new();
        jit.compile_func(&func).expect("jit function");
        assert_eq!(
            run(jit.code(), [20, 1, 0])[0] as i64,
            2_432_902_008_176_640_000
        );
    }

    #[test]
    fn idiv_clobbers_do_not_overwrite_live_values() {
        let mut func = Func::new("div_plus", Id(0), vec![Id(0), Id(1)], Some(Type::Int));
        let params = func.intern_params(vec![Id(0), Id(1)]);
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::BinImm {
                    op: BinOp::IDiv,
                    dst: TypeId {
                        id: Id(2),
                        ty: Type::Int,
                    },
                    lhs: Id(0),
                    imm: 2,
                    span: 0,
                },
                Instr::Bin {
                    op: BinOp::IAdd,
                    dst: TypeId {
                        id: Id(3),
                        ty: Type::Int,
                    },
                    lhs: Id(1),
                    rhs: Id(2),
                    span: 0,
                },
            ],
            term: Some(Terminator::Return {
                value: Some(Id(3)),
                span: 0,
            }),
        });

        let mut jit = Jit::new();
        jit.compile_func(&func).expect("jit function");
        assert_eq!(run(jit.code(), [20, 7, 0])[0], 17);
    }

    #[test]
    fn loads_record_field_from_pointer_arg() {
        let record_ty = Type::record(vec![("first", Type::Int), ("second", Type::Int)]);
        let mut func = Func::new("field", Id(0), vec![Id(0)], Some(Type::Int));
        let params = func.intern_params(vec![Id(0)]);
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![Instr::Load {
                dst: TypeId {
                    id: Id(1),
                    ty: Type::Int,
                },
                base: Id(0),
                offset: 8,
                span: 0,
            }],
            term: Some(Terminator::Return {
                value: Some(Id(1)),
                span: 0,
            }),
        });

        let mut record = [11_u64, 42_u64];
        let mut jit = Jit::new();
        jit.compile_func(&func).expect("jit function");
        assert_eq!(run(jit.code(), [record.as_mut_ptr() as u64, 0, 0])[0], 42);
        drop(record_ty);
    }

    #[test]
    fn takes_nested_record_address_and_loads_field() {
        let record_ty = Type::record(vec![(
            "nested",
            Type::record(vec![("first", Type::Int), ("second", Type::Int)]),
        )]);
        let mut func = Func::new("nested_field", Id(0), vec![Id(0)], Some(Type::Int));
        let params = func.intern_params(vec![Id(0)]);
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::AddrOf {
                    dst: TypeId {
                        id: Id(1),
                        ty: Type::record(vec![("first", Type::Int), ("second", Type::Int)]),
                    },
                    base: Id(0),
                    offset: 8,
                    span: 0,
                },
                Instr::Load {
                    dst: TypeId {
                        id: Id(2),
                        ty: Type::Int,
                    },
                    base: Id(1),
                    offset: 8,
                    span: 0,
                },
            ],
            term: Some(Terminator::Return {
                value: Some(Id(2)),
                span: 0,
            }),
        });

        let mut record = [99_u64, 11_u64, 42_u64];
        let mut jit = Jit::new();
        jit.compile_func(&func).expect("jit function");
        assert_eq!(run(jit.code(), [record.as_mut_ptr() as u64, 0, 0])[0], 42);
        drop(record_ty);
    }

    #[test]
    fn skips_unsupported_functions() {
        let mut func = Func::new("unsupported", Id(0), vec![], Some(Type::Int));
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            instructions: vec![Instr::LoadConst {
                dst: TypeId {
                    id: Id(0),
                    ty: Type::Double,
                },
                value: Const::Double(1.0f64.to_bits()),
                span: 0,
            }],
            params: EMPTY_PARAMS,
            term: Some(Terminator::Return {
                value: Some(Id(0)),
                span: 0,
            }),
        });

        assert!(Jit::new().compile_func(&func).is_none());
    }

    /// Full dispatch path: JIT page injected into syscalls, Call replaced by
    /// Sys, result readable from r0 after vm.run().
    ///
    /// Vm is repr(C) with r as its first field and Value is repr(transparent)
    /// over u64, so rdi is &vm.r[0] the native fn receives &mut Vm. The page
    /// pointer transmutes directly to BuiltinFn, so we need no second native
    /// call mechanism. A bare `ret` leaves r0 untouched.
    #[test]
    fn jit_fn_injected_as_syscall_and_dispatched() {
        use purple_garden_runtime::{Vm, VmConfig, op::Op};

        let jit_fn = super::JitFn::new(&[0xc3]).expect("jit fn");

        let syscalls = vec![jit_fn.entry()];
        let mut vm = Vm::new(VmConfig::default());
        vm.bytecode = vec![Op::LoadI { dst: 0, value: 187 }, Op::Sys { idx: 0 }];
        vm.run(&syscalls).expect("vm run");
        assert_eq!(vm.r(0).as_int(), 187);
    }
}

#[cfg(all(
    test,
    target_arch = "aarch64",
    any(target_os = "linux", target_os = "macos")
))]
mod tests_aarch64 {
    use super::Jit;
    use purple_garden_ir::{Block, Func, Id, Terminator, ptype::Type};

    #[test]
    fn scaffold_falls_back_to_bytecode() {
        let mut func = Func::new("identity", Id(0), vec![Id(0)], Some(Type::Int));
        let params = func.intern_params(vec![Id(0)]);
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            instructions: vec![],
            params,
            term: Some(Terminator::Return {
                value: Some(Id(0)),
                span: 0,
            }),
        });

        let mut jit = Jit::new();
        assert!(jit.compile_func(&func).is_none());
        assert!(jit.code().is_empty());
    }
}
