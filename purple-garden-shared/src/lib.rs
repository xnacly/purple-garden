//! utilities used by multiple purple-garden crates.

use std::ffi::c_void;

#[cfg(not(all(
    any(target_os = "linux", target_os = "macos"),
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
compile_error!("purple-garden-shared currently supports only Linux or macOS on x86_64 or aarch64");

pub mod config;
pub mod mmap;

/// Signature for native VM syscalls and JIT entry points.
///
/// Calling convention:
/// - The argument is an erased pointer to the runtime `Vm`; syscall wrappers
///   cast it back to `*mut Vm` at the boundary.
/// - Args are passed in `r0..r{argcount-1}`. Read them via `vm.r(i)` starting
///   at 0.
/// - `r0` is also the return-value slot. Write the result via
///   `*vm.r_mut(0) = value`. Void functions leave `r0` untouched.
/// - Do not modify any register above `r{argcount-1}`. The bytecode emitter
///   only spills caller-save values in `r0..r{argcount-1}`, relying on this
///   convention to leave `r{argcount}+` untouched. A violation silently
///   corrupts live values in release; debug builds catch it via the runtime
///   dispatcher's syscall register check.
/// - Signal errors via `Vm::trap`; traps are checked at the next `Ret`.
///
/// The pointer is erased so shared IR metadata can name the syscall ABI without
/// depending on the runtime crate's `Vm` type.
pub type BuiltinFn = unsafe extern "C" fn(*mut c_void);

#[cfg(feature = "trace")]
pub mod trace {
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    static START: OnceLock<Instant> = OnceLock::new();

    #[must_use]
    pub fn elapsed() -> Duration {
        START.get_or_init(Instant::now).elapsed()
    }
}

#[cfg(feature = "trace")]
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        let elapsed = $crate::trace::elapsed();
        eprintln!(
            "[{:>12}.{:03}us] {}",
            elapsed.as_micros(),
            elapsed.subsec_nanos() % 1_000,
            format_args!($($arg)*)
        );
    };
}

#[cfg(not(feature = "trace"))]
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {};
}
