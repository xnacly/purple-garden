//! Baseline JIT backend.
//!
//! This is not an optimizing native-code compiler; it is a dispatch remover.
//! [`Jit::compile_func`] lowers a supported IR function straight to native code
//! that reads and writes the VM register file in place. The native ABI passes
//! `*mut Vm` in the first argument register, and because `Vm::r` is the first
//! field of `Vm`, generated code treats that pointer as the base of the VM
//! register slots; scratch native registers (e.g. `rax`) are transient, program
//! values stay in `Vm::r`.

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
        Block, Const, EMPTY_PARAMS, Func, Id, Instr, Terminator, TypeId, ptype::Type,
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
