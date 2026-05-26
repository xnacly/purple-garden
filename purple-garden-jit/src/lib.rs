#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
compile_error!("purple-garden-jit currently supports only Linux on x86_64 or aarch64");

pub mod asm;
pub mod mem;

use purple_garden_ir as ir;

pub use mem::JitFn;

/// The baseline just in time compiler backend state.
#[derive(Default)]
pub struct Bjit {}

impl Bjit {
    pub fn from(ir: &[ir::Func]) {
        #[cfg(target_arch = "x86_64")]
        {
            Self::to_x86(ir);
        }

        #[cfg(target_arch = "aarch64")]
        {
            Self::to_aarch64(ir);
        }
    }

    /// <https://c9x.me/x86>/
    pub fn to_x86(_ir: &[ir::Func]) {
        unimplemented!()
    }

    /// <https://www.cs.swarthmore.edu>/~`kwebb/cs31/resources/ARM64_Cheat_Sheet.pdf`
    pub fn to_aarch64(_ir: &[ir::Func]) {
        unimplemented!()
    }
}

#[cfg(all(test, target_arch = "x86_64", target_os = "linux"))]
mod tests {
    use super::mem::ExecPage;

    #[test]
    fn ret_v0_roundtrips_r0() {
        // fn ret(a: int) int { a } per IR `ret %v0`:
        //   48 8b 07   mov rax, [rdi]   ; r0 -> rax
        //   48 89 07   mov [rdi], rax   ; rax -> r0 (round-trip via native)
        //   c3         ret
        let code: &[u8] = &[0x48, 0x8b, 0x07, 0x48, 0x89, 0x07, 0xc3];
        let page = ExecPage::new(code).expect("mmap");

        let f: unsafe extern "C" fn(*mut u64) = unsafe { std::mem::transmute(page.as_ptr()) };
        let mut regs: [u64; 3] = [187, 0xdead, 0xaffe];
        unsafe { f(regs.as_mut_ptr()) };
        assert_eq!(regs, [187, 0xdead, 0xaffe]);
    }

    /// Full dispatch path: JIT page injected into syscalls,
    /// Call replaced by Sys, result readable from r0 after vm.run().
    ///
    /// Vm is repr(C) with r as its first field and Value is repr(transparent)
    /// over u64, so rdi is &vm.r[0] the native fn receives &mut Vm
    ///
    /// The page pointer transmutes directly to BuiltinFn therefore we dont need a second native
    /// call mechanism
    #[test]
    fn jit_fn_injected_as_syscall_and_dispatched() {
        use purple_garden_runtime::{Vm, VmConfig, op::Op};

        // same instructions as in roundtrip
        let code: &[u8] = &[0x48, 0x8b, 0x07, 0x48, 0x89, 0x07, 0xc3];
        let jit_fn = super::JitFn::new(code).expect("jit fn");

        let syscalls = vec![jit_fn.entry()];
        let mut vm = Vm::new(VmConfig::default());
        vm.bytecode = vec![Op::LoadI { dst: 0, value: 187 }, Op::Sys { idx: 0 }];
        vm.run(&syscalls).expect("vm run");
        assert_eq!(vm.r(0).as_int(), 187);
    }
}
