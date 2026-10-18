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
//! the VM as a syscall entry, the VM enters it through `Op::Native` while Rust
//! builtins stay on `Op::Sys`.
//!
//! The high-level pipeline is:
//!
//! 1. compute shared IR liveness
//! 2. ask the active architecture backend to plan the function (based on support, amount of
//!    registers used, size of function and other heuristics)
//! 3. allocate physical registers from the liveness and target constraints
//! 4. emit native code
//! 5. append the code to the compile's [`CodeArena`]
//!
//! The architecture backend owns the supported IR subset. This crate should
//! therefore be read as a conservative native lowering path, not as a second
//! semantic implementation of the language.
#![feature(allocator_ext)]

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
mod xralloc2;

pub use mem::CodeArena;
use purple_garden_ir as ir;
use purple_garden_runtime::{BuiltinFn, Value};
use std::{
    alloc::{Allocator, Global},
    collections::HashMap,
    hash::RandomState,
};

/// Entry points of the functions already compiled natively, in any allocator.
pub type Natives<A> = HashMap<ir::Id, BuiltinFn, RandomState, A>;

/// Reusable JIT codegen state, every buffer lives in the scratch allocator `S`.
#[derive(Debug, Clone)]
pub struct Jit<S: Allocator + Clone = Global> {
    code: Vec<u8, S>,
    liveness: Vec<(u32, u32), S>,
    regalloc: xralloc2::Xralloc2<S>,
    scratch: arch::Scratch<S>,
}

impl Default for Jit {
    fn default() -> Self {
        Self::new_in(Global)
    }
}

impl Jit {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<S: Allocator + Clone> Jit<S> {
    #[must_use]
    pub fn new_in(scratch: S) -> Self {
        Self {
            code: Vec::new_in(scratch.clone()),
            liveness: Vec::new_in(scratch.clone()),
            regalloc: xralloc2::Xralloc2::new_in(scratch.clone()),
            scratch: arch::Scratch::new_in(scratch),
        }
    }

    /// Lower and encode `func`, returning `None` when unsupported.
    pub fn compile_func<F: Allocator>(&mut self, func: &ir::Func<'_, F>) -> Option<()> {
        self.liveness.clear();
        func.live_set_into(&mut self.liveness);
        let scratch = self.liveness.allocator().clone();
        let liveness = std::mem::replace(&mut self.liveness, Vec::new_in(scratch));
        let result =
            self.compile_func_with_liveness(func, &liveness, &HashMap::new(), &[], &HashMap::new());
        self.liveness = liveness;
        result
    }

    /// Lower and encode `func` using precomputed liveness.
    /// `globals` maps constants to their `vm.globals` slot, `strings[slot]` is
    /// the final address of a string constant native code embeds. `natives`
    /// holds the entry points of the functions already compiled natively.
    pub fn compile_func_with_liveness<'ir, F: Allocator>(
        &mut self,
        func: &ir::Func<'ir, F>,
        liveness: &[(u32, u32)],
        globals: &HashMap<ir::Const<'ir>, u32>,
        strings: &[Value],
        natives: &Natives<impl Allocator>,
    ) -> Option<()> {
        self.code.clear();
        let result = arch::compile_func(
            func,
            &mut self.code,
            liveness,
            globals,
            strings,
            natives,
            &mut self.regalloc,
            &mut self.scratch,
        );
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
    use super::mem::CodeArena;
    use purple_garden_ir::{
        BinOp, Block, Const, EMPTY_PARAMS, Func, Id, Instr, Terminator, TypeId,
        ptype::{Field, Type},
    };

    fn record(fields: Vec<(&'static str, Type<'static>)>) -> Type<'static> {
        Type::Record(
            fields
                .into_iter()
                .map(|(name, ty)| Field { name, ty })
                .collect::<Vec<_>>()
                .leak(),
        )
    }

    /// `code` made executable, the arena has to outlive every call.
    fn exec(code: &[u8]) -> (CodeArena, unsafe extern "C" fn(*mut u64)) {
        let mut arena = CodeArena::new().expect("code arena");
        let entry = arena.push(code).expect("code fits the arena");
        arena.seal().expect("seal the code arena");
        let f = unsafe {
            std::mem::transmute::<super::BuiltinFn, unsafe extern "C" fn(*mut u64)>(entry)
        };
        (arena, f)
    }

    /// Enter native code from a real `vm` the way the interpreter does, so a trap
    /// can leave through `jit_abort`.
    fn enter(code: &[u8], vm: &mut purple_garden_runtime::Vm) {
        let mut arena = CodeArena::new().expect("code arena");
        let entry = arena.push(code).expect("code fits the arena");
        arena.seal().expect("seal the code arena");
        unsafe {
            purple_garden_runtime::enter_native(
                (vm as *mut purple_garden_runtime::Vm).cast(),
                entry,
            )
        };
    }

    /// Run native code that takes `*mut u64` (the VM register file) and return
    /// the resulting register slots.
    fn run(code: &[u8], mut regs: [u64; 3]) -> [u64; 3] {
        let (_arena, f) = exec(code);
        unsafe { f(regs.as_mut_ptr()) };
        regs
    }

    /// `fn identity(a) int { a }`: load the arg, store it back as the result.
    #[test]
    fn identity_returns_arg() {
        let mut func = Func::new(
            "identity",
            Id(0),
            vec![TypeId {
                id: Id(0),
                ty: Type::Int,
            }],
            Some(Type::Int),
        );
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
        assert_eq!(jit.code(), [0x31, 0xc0, 0xc3]); // xor eax,eax; ret
        assert_eq!(run(jit.code(), [42, 0xdead, 0xaffe]), [42, 0xdead, 0xaffe]);
    }

    /// `fn second(a b) int { b }`: the result is the second arg (vm.r[1]).
    #[test]
    fn returns_second_arg() {
        let mut func = Func::new(
            "second",
            Id(0),
            vec![
                TypeId {
                    id: Id(0),
                    ty: Type::Int,
                },
                TypeId {
                    id: Id(1),
                    ty: Type::Int,
                },
            ],
            Some(Type::Int),
        );
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
        assert_eq!(
            jit.code(),
            [
                0x48, 0x8b, 0x77, 0x00, // mov rsi,[rdi+0]
                0x48, 0x8b, 0x77, 0x08, // mov rsi,[rdi+8]
                0x48, 0x89, 0x77, 0x00, // mov [rdi+0],rsi
                0x31, 0xc0, // xor eax,eax: no trap
                0xc3,
            ]
        );
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

    /// Allocation is a helper call: values used after it must survive the
    /// caller-saved register clobber. This also exercises the VM-pointer ABI
    /// and writing the returned payload.
    #[test]
    fn alloc_preserves_live_values_across_helper_call() {
        use purple_garden_runtime::{Vm, VmConfig};
        use std::alloc::Layout;

        let record_ty = record(vec![
            ("r", Type::Int),
            ("g", Type::Int),
            ("b", Type::Int),
            ("a", Type::Int),
        ]);
        let mut func = Func::new(
            "alloc_live",
            Id(0),
            vec![
                TypeId {
                    id: Id(0),
                    ty: Type::Int,
                },
                TypeId {
                    id: Id(1),
                    ty: Type::Int,
                },
                TypeId {
                    id: Id(2),
                    ty: Type::Int,
                },
            ],
            Some(record_ty),
        );
        let params = func.intern_params(vec![Id(0), Id(1), Id(2)]);
        let alloc_id = Id(3);
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::Alloc {
                    dst: TypeId {
                        id: alloc_id,
                        ty: record_ty,
                    },
                    layout: Layout::from_size_align(32, 8).unwrap(),
                    span: 0,
                },
                Instr::Store {
                    src: Id(0),
                    base: alloc_id,
                    offset: 0,
                    span: 0,
                },
                Instr::Store {
                    src: Id(1),
                    base: alloc_id,
                    offset: 8,
                    span: 0,
                },
                Instr::Store {
                    src: Id(2),
                    base: alloc_id,
                    offset: 16,
                    span: 0,
                },
                Instr::LoadConst {
                    dst: TypeId {
                        id: Id(4),
                        ty: Type::Int,
                    },
                    value: Const::Int(255),
                    span: 0,
                },
                Instr::Store {
                    src: Id(4),
                    base: alloc_id,
                    offset: 24,
                    span: 0,
                },
            ],
            term: Some(Terminator::Return {
                value: Some(alloc_id),
                span: 0,
            }),
        });

        let mut jit = Jit::new();
        jit.compile_func(&func).expect("jit function");
        let mut vm = Vm::new(VmConfig {
            no_gc: true,
            ..VmConfig::default()
        });
        let slots = unsafe { &mut *(&mut vm as *mut Vm as *mut [u64; 64]) };
        slots[0..3].copy_from_slice(&[1, 2, 3]);
        let (_arena, f) = exec(jit.code());
        unsafe { f(&mut vm as *mut Vm as *mut u64) };
        let payload = unsafe { std::slice::from_raw_parts(slots[0] as *const u64, 4) };
        assert_eq!(payload, &[1, 2, 3, 255]);
    }

    #[test]
    fn compiles_tail_recursive_factorial_loop() {
        let mut func = Func::new(
            "factorial",
            Id(0),
            vec![
                TypeId {
                    id: Id(0),
                    ty: Type::Int,
                },
                TypeId {
                    id: Id(1),
                    ty: Type::Int,
                },
            ],
            Some(Type::Int),
        );
        let args0 = func.intern_params([Id(5), Id(6)]);
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
                    args: args0,
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
        let mut func = Func::new(
            "div_plus",
            Id(0),
            vec![
                TypeId {
                    id: Id(0),
                    ty: Type::Int,
                },
                TypeId {
                    id: Id(1),
                    ty: Type::Int,
                },
            ],
            Some(Type::Int),
        );
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

    /// `fn f(a..f) int { a / 3 + a + .. + f }`: every param stays live across
    /// the idiv, so its result lands in rcx or rdx, which the save around the
    /// idiv must not restore over.
    #[test]
    fn imm_divisor_result_survives_clobber_restore() {
        let int = |id| TypeId {
            id: Id(id),
            ty: Type::Int,
        };
        let ids: Vec<_> = (0..6).map(Id).collect();
        let mut func = Func::new(
            "div_sum",
            Id(0),
            ids.iter().map(|&id| TypeId { id, ty: Type::Int }).collect(),
            Some(Type::Int),
        );
        let params = func.intern_params(ids);
        let mut instructions = vec![Instr::BinImm {
            op: BinOp::IDiv,
            dst: int(6),
            lhs: Id(0),
            imm: 3,
            span: 0,
        }];
        for param in 0..6 {
            instructions.push(Instr::Bin {
                op: BinOp::IAdd,
                dst: int(7 + param),
                lhs: Id(6 + param),
                rhs: Id(param),
                span: 0,
            });
        }
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions,
            term: Some(Terminator::Return {
                value: Some(Id(12)),
                span: 0,
            }),
        });

        let mut jit = Jit::new();
        jit.compile_func(&func).expect("jit function");
        let (_arena, f) = exec(jit.code());
        let mut regs = [30, 1, 2, 3, 4, 5];
        unsafe { f(regs.as_mut_ptr()) };
        assert_eq!(regs[0], 10 + 30 + 1 + 2 + 3 + 4 + 5);
    }

    /// Power of two divisors lower to shifts, which have to truncate toward zero like the VM.
    #[test]
    fn pow2_divisors_truncate_like_the_vm() {
        for op in [BinOp::IDiv, BinOp::IMod] {
            for imm in [2, 16, 1 << 30] {
                let mut func = Func::new(
                    "pow2",
                    Id(0),
                    vec![TypeId {
                        id: Id(0),
                        ty: Type::Int,
                    }],
                    Some(Type::Int),
                );
                let params = func.intern_params([Id(0)]);
                func.blocks.push(Block {
                    tombstone: false,
                    id: Id(0),
                    params,
                    instructions: vec![Instr::BinImm {
                        op,
                        dst: TypeId {
                            id: Id(1),
                            ty: Type::Int,
                        },
                        lhs: Id(0),
                        imm,
                        span: 0,
                    }],
                    term: Some(Terminator::Return {
                        value: Some(Id(1)),
                        span: 0,
                    }),
                });

                let mut jit = Jit::new();
                jit.compile_func(&func).expect("jit pow2 divisor");
                for x in [i64::MIN, -33, -17, -16, -3, -1, 0, 1, 3, 16, 33, i64::MAX] {
                    let expected = match op {
                        BinOp::IDiv => x / imm as i64,
                        _ => x % imm as i64,
                    };
                    assert_eq!(
                        run(jit.code(), [x as u64, 0, 0])[0] as i64,
                        expected,
                        "{x} {op:?} {imm}"
                    );
                }
            }
        }
    }

    /// `fn f(a:Bool b:Bool) Bool { a == b }`
    #[test]
    fn bool_equality() {
        let bool = |id| TypeId {
            id: Id(id),
            ty: Type::Bool,
        };
        let mut func = Func::new("beq", Id(0), vec![bool(0), bool(1)], Some(Type::Bool));
        let params = func.intern_params([Id(0), Id(1)]);
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![Instr::Bin {
                op: BinOp::BEq,
                dst: bool(2),
                lhs: Id(0),
                rhs: Id(1),
                span: 0,
            }],
            term: Some(Terminator::Return {
                value: Some(Id(2)),
                span: 0,
            }),
        });

        let mut jit = Jit::new();
        jit.compile_func(&func).expect("jit beq");
        for (a, b) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
            assert_eq!(
                run(jit.code(), [a, b, 0])[0],
                u64::from(a == b),
                "{a} == {b}"
            );
        }
    }

    unsafe extern "C" fn no_wrapper(_: *mut std::ffi::c_void) {
        unreachable!("native code calls the native entry")
    }

    unsafe extern "C" fn mix(_: *mut std::ffi::c_void, a: u64, b: f64) -> f64 {
        a as i64 as f64 * b
    }

    unsafe extern "C" fn add_one(_: *mut std::ffi::c_void, a: u64) -> u64 {
        a + 1
    }

    static MIX: purple_garden_ir::Fn<'static> = purple_garden_ir::Fn {
        name: "mix",
        doc: "",
        ptr: no_wrapper,
        pure: false,
        eval: None,
        arg_names: &["a", "b"],
        args: &[Type::Int, Type::Double],
        ret: Type::Double,
        specialises: None,
        with_slots: false,
        intrinsic: None,
        native: Some(unsafe {
            std::mem::transmute::<
                unsafe extern "C" fn(*mut std::ffi::c_void, u64, f64) -> f64,
                purple_garden_ir::NativeFn,
            >(mix)
        }),
    };

    static ADD_ONE: purple_garden_ir::Fn<'static> = purple_garden_ir::Fn {
        name: "add_one",
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
        native: Some(unsafe {
            std::mem::transmute::<
                unsafe extern "C" fn(*mut std::ffi::c_void, u64) -> u64,
                purple_garden_ir::NativeFn,
            >(add_one)
        }),
    };

    /// Whether `code` calls `jit_sys`, the path through `vm.r` native entries replace.
    fn calls_jit_sys(code: &[u8]) -> bool {
        let jit_sys = purple_garden_runtime::jit_sys as *const () as u64;
        code.windows(8).any(|w| w == jit_sys.to_le_bytes())
    }

    /// `fn f(x:Int y:Double) Double { mix(x y) }`: the Int in a gpr, the Double in an xmm
    /// register, the result back in xmm0.
    #[test]
    fn calls_native_entries_with_hardware_args_and_result() {
        let mut func = Func::new(
            "f",
            Id(0),
            vec![
                TypeId {
                    id: Id(0),
                    ty: Type::Int,
                },
                TypeId {
                    id: Id(1),
                    ty: Type::Double,
                },
            ],
            Some(Type::Double),
        );
        let params = func.intern_params([Id(0), Id(1)]);
        let args = func.intern_params([Id(0), Id(1)]);
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![Instr::Sys {
                dst: TypeId {
                    id: Id(2),
                    ty: Type::Double,
                },
                path: "t",
                fun: &MIX,
                args,
                span: 0,
            }],
            term: Some(Terminator::Return {
                value: Some(Id(2)),
                span: 0,
            }),
        });

        let mut jit = Jit::new();
        jit.compile_func(&func).expect("jit native entry call");
        assert!(!calls_jit_sys(jit.code()));
        assert_eq!(
            f64::from_bits(run(jit.code(), [3, 1.5f64.to_bits(), 0])[0]),
            4.5
        );
    }

    /// `fn g(x:Int) Int { add_one(x) + x }`: x survives the call in a register the callee keeps.
    #[test]
    fn native_entry_calls_keep_live_values() {
        let int = |id| TypeId {
            id: Id(id),
            ty: Type::Int,
        };
        let mut func = Func::new("g", Id(0), vec![int(0)], Some(Type::Int));
        let params = func.intern_params([Id(0)]);
        let args = func.intern_params([Id(0)]);
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::Sys {
                    dst: int(1),
                    path: "t",
                    fun: &ADD_ONE,
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
            ],
            term: Some(Terminator::Return {
                value: Some(Id(2)),
                span: 0,
            }),
        });

        let mut jit = Jit::new();
        jit.compile_func(&func).expect("jit native entry call");
        assert!(!calls_jit_sys(jit.code()));
        assert_eq!(run(jit.code(), [5, 0, 0])[0], 11);
    }

    /// `fn f(a, b) int { a <op> b }` with a register divisor.
    fn reg_div_func(op: BinOp) -> Func<'static> {
        let mut func = Func::new(
            "reg_div",
            Id(0),
            vec![
                TypeId {
                    id: Id(0),
                    ty: Type::Int,
                },
                TypeId {
                    id: Id(1),
                    ty: Type::Int,
                },
            ],
            Some(Type::Int),
        );
        let params = func.intern_params(vec![Id(0), Id(1)]);
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![Instr::Bin {
                op,
                dst: TypeId {
                    id: Id(2),
                    ty: Type::Int,
                },
                lhs: Id(0),
                rhs: Id(1),
                span: 0,
            }],
            term: Some(Terminator::Return {
                value: Some(Id(2)),
                span: 0,
            }),
        });
        func
    }

    #[test]
    fn reg_divisor_imod_and_idiv() {
        let mut jit = Jit::new();
        jit.compile_func(&reg_div_func(BinOp::IMod))
            .expect("jit imod");
        assert_eq!(run(jit.code(), [20, 7, 0])[0], 6);
        assert_eq!(run(jit.code(), [7, 20, 0])[0], 7);
        assert_eq!(run(jit.code(), [-20i64 as u64, 7, 0])[0] as i64, -6);

        let mut jit = Jit::new();
        jit.compile_func(&reg_div_func(BinOp::IDiv))
            .expect("jit idiv");
        assert_eq!(run(jit.code(), [20, 7, 0])[0], 2);
        assert_eq!(run(jit.code(), [-20i64 as u64, 7, 0])[0] as i64, -2);
    }

    /// `fn f(a, b) int { b + (a % b) }`: `b` must survive the rax/rcx/rdx
    /// clobber from the register-divisor idiv.
    #[test]
    fn reg_divisor_clobbers_do_not_overwrite_live_values() {
        let mut func = Func::new(
            "mod_plus",
            Id(0),
            vec![
                TypeId {
                    id: Id(0),
                    ty: Type::Int,
                },
                TypeId {
                    id: Id(1),
                    ty: Type::Int,
                },
            ],
            Some(Type::Int),
        );
        let params = func.intern_params(vec![Id(0), Id(1)]);
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::Bin {
                    op: BinOp::IMod,
                    dst: TypeId {
                        id: Id(2),
                        ty: Type::Int,
                    },
                    lhs: Id(0),
                    rhs: Id(1),
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
        assert_eq!(run(jit.code(), [20, 7, 0])[0], 13);
    }

    /// A zero register divisor must take the trap path and return, not fault.
    /// Needs a real `Vm` because the trap helper writes `pending_trap`.
    #[test]
    fn reg_divisor_zero_traps_instead_of_faulting() {
        use purple_garden_runtime::{Anomaly, Vm, VmConfig};

        let mut jit = Jit::new();
        jit.compile_func(&reg_div_func(BinOp::IMod))
            .expect("jit imod");

        let mut vm = Vm::new(VmConfig::default());
        let slots = unsafe { &mut *(&mut vm as *mut Vm as *mut [u64; 64]) };
        slots[0..2].copy_from_slice(&[20, 0]);
        enter(jit.code(), &mut vm);

        assert!(matches!(
            vm.take_trap(),
            Some(Anomaly::DivisionByZero { .. })
        ));
    }

    /// `fn f(c:Bool) Bool { testing.assert(c) c }`, a false `c` leaves through `jit_abort`.
    #[test]
    fn assert_traps_on_false() {
        use purple_garden_runtime::{Anomaly, Vm, VmConfig};

        let mut func = Func::new(
            "assert",
            Id(0),
            vec![TypeId {
                id: Id(0),
                ty: Type::Bool,
            }],
            Some(Type::Bool),
        );
        let params = func.intern_params([Id(0)]);
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![Instr::Assert {
                cond: Id(0),
                span: 0,
            }],
            term: Some(Terminator::Return {
                value: Some(Id(0)),
                span: 0,
            }),
        });

        let mut jit = Jit::new();
        jit.compile_func(&func).expect("jit assert");
        for (cond, traps) in [(1, false), (0, true)] {
            let mut vm = Vm::new(VmConfig::default());
            *vm.r_mut(0) = purple_garden_runtime::Value(cond);
            enter(jit.code(), &mut vm);
            assert_eq!(
                matches!(vm.take_trap(), Some(Anomaly::AssertionFailed { .. })),
                traps,
                "assert({cond})"
            );
        }
    }

    #[test]
    fn loads_record_field_from_pointer_arg() {
        let record_ty = record(vec![("first", Type::Int), ("second", Type::Int)]);
        let mut func = Func::new(
            "field",
            Id(0),
            vec![TypeId {
                id: Id(0),
                ty: record_ty.clone(),
            }],
            Some(Type::Int),
        );
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
        let _ = record_ty;
    }

    #[test]
    fn takes_nested_record_address_and_loads_field() {
        let record_ty = record(vec![(
            "nested",
            record(vec![("first", Type::Int), ("second", Type::Int)]),
        )]);
        let mut func = Func::new(
            "nested_field",
            Id(0),
            vec![TypeId {
                id: Id(0),
                ty: record_ty.clone(),
            }],
            Some(Type::Int),
        );
        let params = func.intern_params(vec![Id(0)]);
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![
                Instr::AddrOf {
                    dst: TypeId {
                        id: Id(1),
                        ty: record(vec![("first", Type::Int), ("second", Type::Int)]),
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
        let _ = record_ty;
    }

    /// `fn f() Double { 3.1415 }`: the bits are staged through rax into an
    /// xmm, which the return stores to vm.r[0] as the raw double.
    #[test]
    fn compiles_double_const_returning_function() {
        let mut func = Func::new("double_ret", Id(0), vec![], Some(Type::Double));
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            instructions: vec![Instr::LoadConst {
                dst: TypeId {
                    id: Id(0),
                    ty: Type::Double,
                },
                value: Const::Double(3.1415f64.to_bits()),
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
        let mut expected = vec![0x48, 0xb8]; // movabs rax, bits
        expected.extend_from_slice(&3.1415f64.to_bits().to_le_bytes());
        expected.extend_from_slice(&[
            0x66, 0x48, 0x0f, 0x6e, 0xc0, // movq xmm0, rax
            0xf2, 0x0f, 0x11, 0x47, 0x00, // movsd [rdi+0], xmm0
            0x31, 0xc0, // xor eax, eax: no trap
            0xc3,
        ]);
        assert_eq!(jit.code(), expected);
        assert_eq!(f64::from_bits(run(jit.code(), [0, 0, 0])[0]), 3.1415);
    }

    /// Tail calls to other functions aren't lowered yet, so a function using them stays bytecode.
    #[test]
    fn skips_unsupported_functions() {
        let mut func = Func::new("unsupported", Id(0), vec![], Some(Type::Int));
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            instructions: vec![],
            params: EMPTY_PARAMS,
            term: Some(Terminator::Tail {
                func: Id(1),
                args: EMPTY_PARAMS,
                span: 0,
            }),
        });

        assert!(Jit::new().compile_func(&func).is_none());
    }

    /// `fn f() Double { lhs <op> rhs }`, the operands loaded as constants since
    /// double params aren't lowered yet.
    fn double_bin_func(op: BinOp, lhs: f64, rhs: f64) -> Func<'static> {
        let double = |id| TypeId {
            id: Id(id),
            ty: Type::Double,
        };
        let mut func = Func::new("double_bin", Id(0), vec![], Some(Type::Double));
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            instructions: vec![
                Instr::LoadConst {
                    dst: double(0),
                    value: Const::Double(lhs.to_bits()),
                    span: 0,
                },
                Instr::LoadConst {
                    dst: double(1),
                    value: Const::Double(rhs.to_bits()),
                    span: 0,
                },
                Instr::Bin {
                    op,
                    dst: double(2),
                    lhs: Id(0),
                    rhs: Id(1),
                    span: 0,
                },
            ],
            params: EMPTY_PARAMS,
            term: Some(Terminator::Return {
                value: Some(Id(2)),
                span: 0,
            }),
        });
        func
    }

    #[test]
    fn double_bin_ops() {
        if !std::arch::is_x86_feature_detected!("avx") {
            return;
        }
        for (op, expected) in [
            (BinOp::DAdd, 9.0),
            (BinOp::DSub, 5.0),
            (BinOp::DMul, 14.0),
            (BinOp::DDiv, 3.5),
        ] {
            let mut jit = Jit::new();
            jit.compile_func(&double_bin_func(op, 7.0, 2.0))
                .expect("jit double bin op");
            assert_eq!(f64::from_bits(run(jit.code(), [0, 0, 0])[0]), expected);
        }
    }

    /// Every NaN compare is false, as in the VM.
    #[test]
    fn double_compares() {
        for (op, lhs, rhs, expected) in [
            (BinOp::DLt, 1.0, 2.0, 1),
            (BinOp::DLt, 2.0, 1.0, 0),
            (BinOp::DLt, 1.0, 1.0, 0),
            (BinOp::DLt, f64::NAN, 1.0, 0),
            (BinOp::DLt, 1.0, f64::NAN, 0),
            (BinOp::DGt, 2.0, 1.0, 1),
            (BinOp::DGt, 1.0, 2.0, 0),
            (BinOp::DGt, 1.0, 1.0, 0),
            (BinOp::DGt, f64::NAN, 1.0, 0),
            (BinOp::DGt, 1.0, f64::NAN, 0),
        ] {
            let mut func = double_bin_func(op, lhs, rhs);
            func.blocks[0].instructions[2] = Instr::Bin {
                op,
                dst: TypeId {
                    id: Id(2),
                    ty: Type::Bool,
                },
                lhs: Id(0),
                rhs: Id(1),
                span: 0,
            };
            let mut jit = Jit::new();
            jit.compile_func(&func).expect("jit double compare");
            assert_eq!(
                run(jit.code(), [0, 0, 0])[0],
                expected,
                "{lhs} {op:?} {rhs}"
            );
        }
    }

    /// `fn f(x:<from>) <to> { x as <to> }`
    fn cast_func(from: Type<'static>, to: Type<'static>) -> Func<'static> {
        let mut func = Func::new(
            "cast",
            Id(0),
            vec![TypeId {
                id: Id(0),
                ty: from.clone(),
            }],
            Some(to.clone()),
        );
        let params = func.intern_params([Id(0)]);
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![Instr::Cast {
                dst: TypeId { id: Id(1), ty: to },
                from: TypeId {
                    id: Id(0),
                    ty: from,
                },
                span: 0,
            }],
            term: Some(Terminator::Return {
                value: Some(Id(1)),
                span: 0,
            }),
        });
        func
    }

    /// Like the VM's `as i64`: truncates toward zero, saturates out of range and maps NaN to 0.
    #[test]
    fn casts_double_to_int() {
        let mut jit = Jit::new();
        jit.compile_func(&cast_func(Type::Double, Type::Int))
            .expect("jit double to int");
        for x in [
            0.0,
            -0.0,
            3.9,
            -3.9,
            0.5,
            -0.5,
            9007199254740993.0,
            9223372036854775807.0,
            -9223372036854775808.0,
            1e19,
            -1e19,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
        ] {
            assert_eq!(
                run(jit.code(), [x.to_bits(), 0, 0])[0] as i64,
                x as i64,
                "{x} as Int"
            );
        }
    }

    #[test]
    fn casts_int_to_bool() {
        let mut jit = Jit::new();
        jit.compile_func(&cast_func(Type::Int, Type::Bool))
            .expect("jit int to bool");
        for x in [0, 1, -1, 42, i64::MIN, i64::MAX] {
            assert_eq!(
                run(jit.code(), [x as u64, 0, 0])[0],
                u64::from(x != 0),
                "{x} as Bool"
            );
        }
    }

    #[test]
    fn casts_bool_to_int() {
        let mut jit = Jit::new();
        jit.compile_func(&cast_func(Type::Bool, Type::Int))
            .expect("jit bool to int");
        for x in [0, 1] {
            assert_eq!(run(jit.code(), [x, 0, 0])[0], x, "{x} as Int");
        }
    }

    /// `fn f(n:Int) Double { n as Double }`
    #[test]
    fn casts_int_to_double() {
        let mut func = Func::new(
            "cast",
            Id(0),
            vec![TypeId {
                id: Id(0),
                ty: Type::Int,
            }],
            Some(Type::Double),
        );
        let params = func.intern_params([Id(0)]);
        func.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![Instr::Cast {
                dst: TypeId {
                    id: Id(1),
                    ty: Type::Double,
                },
                from: TypeId {
                    id: Id(0),
                    ty: Type::Int,
                },
                span: 0,
            }],
            term: Some(Terminator::Return {
                value: Some(Id(1)),
                span: 0,
            }),
        });

        let mut jit = Jit::new();
        jit.compile_func(&func).expect("jit cast");
        for n in [0i64, -7, 1 << 53] {
            assert_eq!(
                f64::from_bits(run(jit.code(), [n as u64, 0, 0])[0]),
                n as f64
            );
        }
    }

    /// `fn f(n:Int a:Double b:Double) Double { match { n == 0 { a - b } { f(n - 1 b a) } } }`:
    /// the tail call swaps a and b, a cycle only the xmm scratch breaks, and the join's params
    /// are doubles only through the values its edge passes.
    #[test]
    fn double_tail_loop_swaps_args() {
        let ty = |id, ty| TypeId { id: Id(id), ty };
        let mut func = Func::new(
            "swap",
            Id(0),
            vec![ty(0, Type::Int), ty(1, Type::Double), ty(2, Type::Double)],
            Some(Type::Double),
        );
        let params = func.intern_params([Id(0), Id(1), Id(2)]);
        let done_args = func.intern_params([Id(1), Id(2)]);
        let done_params = func.intern_params([Id(4), Id(5)]);
        let swapped = func.intern_params([Id(7), Id(2), Id(1)]);
        func.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params,
                instructions: vec![Instr::BinImm {
                    op: BinOp::IEq,
                    dst: ty(3, Type::Bool),
                    lhs: Id(0),
                    imm: 0,
                    span: 0,
                }],
                term: Some(Terminator::Branch {
                    cond: Id(3),
                    yes: (Id(1), done_args),
                    no: (Id(2), params),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params: done_params,
                instructions: vec![Instr::Bin {
                    op: BinOp::DSub,
                    dst: ty(6, Type::Double),
                    lhs: Id(4),
                    rhs: Id(5),
                    span: 0,
                }],
                term: Some(Terminator::Return {
                    value: Some(Id(6)),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(2),
                params,
                instructions: vec![Instr::BinImm {
                    op: BinOp::ISub,
                    dst: ty(7, Type::Int),
                    lhs: Id(0),
                    imm: 1,
                    span: 0,
                }],
                term: Some(Terminator::Tail {
                    func: Id(0),
                    args: swapped,
                    span: 0,
                }),
            },
        ];

        if !std::arch::is_x86_feature_detected!("avx") {
            return;
        }
        let mut jit = Jit::new();
        jit.compile_func(&func).expect("jit double loop");
        let run = |n: u64| f64::from_bits(run(jit.code(), [n, 5f64.to_bits(), 2f64.to_bits()])[0]);
        assert_eq!(run(0), 3.0);
        assert_eq!(run(3), -3.0);
        assert_eq!(run(4), 3.0);
    }

    /// IEEE 754, like the VM: no trap, the zero's sign picks the infinity.
    #[test]
    fn double_div_by_zero_is_inf() {
        if !std::arch::is_x86_feature_detected!("avx") {
            return;
        }
        for (zero, expected) in [(0.0, f64::INFINITY), (-0.0, f64::NEG_INFINITY)] {
            let mut jit = Jit::new();
            jit.compile_func(&double_bin_func(BinOp::DDiv, 1.0, zero))
                .expect("jit ddiv");
            assert_eq!(f64::from_bits(run(jit.code(), [0, 0, 0])[0]), expected);
        }
    }

    /// Full dispatch path: JIT page injected into syscalls, Call replaced by
    /// Native, result readable from r0 after vm.run().
    ///
    /// Vm is repr(C) with r as its first field and Value is repr(transparent)
    /// over u64, so rdi is &vm.r[0] the native fn receives &mut Vm. The page
    /// pointer transmutes directly to BuiltinFn, entered through
    /// `enter_native`. A bare `ret` leaves r0 untouched.
    #[test]
    fn jit_fn_injected_as_syscall_and_dispatched() {
        use purple_garden_runtime::{Vm, VmConfig, op::Op};

        let mut arena = super::CodeArena::new().expect("code arena");
        let entry = arena.push(&[0xc3]).expect("arena has room");
        arena.seal().expect("seal arena");

        let syscalls = vec![entry];
        let mut vm = Vm::new(VmConfig::default());
        vm.bytecode = vec![
            Op::LoadI { dst: 0, value: 187 },
            Op::Native { idx: 0 },
            Op::Halt,
        ];
        vm.run::<false>(&syscalls).expect("vm run");
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
