pub mod asm;
mod consts;

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

    /// https://c9x.me/x86/
    pub fn to_x86(_ir: &[ir::Func]) {
        unimplemented!()
    }

    /// https://www.cs.swarthmore.edu/~kwebb/cs31/resources/ARM64_Cheat_Sheet.pdf
    pub fn to_aarch64(_ir: &[ir::Func]) {
        unimplemented!()
    }
}
