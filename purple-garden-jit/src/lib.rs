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
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
compile_error!("purple-garden-jit currently supports only Linux on x86_64 or aarch64");

#[cfg(target_arch = "x86_64")]
#[path = "x86/mod.rs"]
mod arch;
#[cfg(target_arch = "aarch64")]
#[path = "aarch64/mod.rs"]
mod arch;
pub mod mem;

pub use mem::JitFn;
use purple_garden_ir as ir;

/// Holds the native-code buffer reused across functions. Each `compile_func`
/// refills it; [`JitFn::new`] copies the bytes into the executable page, so the
/// buffer can be overwritten on the next call.
#[derive(Debug, Default, Clone)]
pub struct Jit {
    code: Vec<u8>,
}

impl Jit {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Lower `func` (with the backend's register assignment `regs`) into the
    /// reusable buffer, returning the bytes, or `None` if unsupported. The
    /// slice is valid until the next call.
    pub fn compile_func(&mut self, func: &ir::Func<'_>, regs: &[ir::Location]) -> Option<&[u8]> {
        self.code.clear();
        arch::compile_func(func, regs, &mut self.code)?;
        Some(&self.code)
    }
}

#[cfg(all(test, target_arch = "x86_64", target_os = "linux"))]
mod tests_x86 {
    use super::Jit;
    use super::mem::ExecPage;
    use purple_garden_ir::{
        Block, Const, EMPTY_PARAMS, Func, Id, Instr, Location, Terminator, TypeId, ptype::Type,
    };

    /// Run native code that takes `*mut u64` (the VM register file) and return
    /// the resulting register slots.
    fn run(code: &[u8], mut regs: [u64; 3]) -> [u64; 3] {
        let page = ExecPage::new(code).expect("mmap");
        let f: unsafe extern "C" fn(*mut u64) = unsafe { std::mem::transmute(page.as_ptr()) };
        unsafe { f(regs.as_mut_ptr()) };
        regs
    }

    /// `fn identity(a) int { a }`: value already in r0, so just `ret`.
    #[test]
    fn identity_is_bare_ret() {
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

        let code = Jit::new()
            .compile_func(&func, &[Location::Reg(0)])
            .expect("jit function")
            .to_vec();
        assert_eq!(code, &[0xc3]);
        assert_eq!(run(&code, [42, 0xdead, 0xaffe]), [42, 0xdead, 0xaffe]);
    }

    /// `fn second(a b) int { b }`: return value lives in r1, must move to r0.
    #[test]
    fn returns_non_r0_param_via_move() {
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

        let code = Jit::new()
            .compile_func(&func, &[Location::Reg(0), Location::Reg(1)])
            .expect("jit function")
            .to_vec();
        assert_eq!(run(&code, [10, 20, 0]), [20, 20, 0]);
    }

    #[test]
    fn skips_non_return_only_functions() {
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

        assert!(Jit::new().compile_func(&func, &[]).is_none());
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

        assert!(Jit::new().compile_func(&func, &[]).is_none());
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
