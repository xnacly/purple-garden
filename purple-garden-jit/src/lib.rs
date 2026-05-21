pub mod asm;
pub mod mem;

use purple_garden_ir as ir;

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
}
