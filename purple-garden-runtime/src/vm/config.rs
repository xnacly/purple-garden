use crate::{DEFAULT_STACK_SIZE, MIB};

use super::CallFrame;

#[derive(Clone, Copy, Debug)]
pub struct VmConfig {
    pub backtrace: bool,
    pub no_gc: bool,
    /// Size of the call stack in MiB, see [`frames_in`].
    pub stack_size: usize,
}

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            backtrace: false,
            no_gc: false,
            stack_size: DEFAULT_STACK_SIZE,
        }
    }
}

/// Call depth `mib` MiB holds; debug frames are twice as wide as release ones.
#[must_use]
pub const fn frames_in(mib: usize) -> usize {
    mib.saturating_mul(MIB) / size_of::<CallFrame>()
}
