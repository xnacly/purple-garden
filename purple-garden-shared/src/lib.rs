//! utilities used by multiple purple-garden crates.

#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
compile_error!("purple-garden-shared currently supports only Linux on x86_64 or aarch64");

pub mod config;
pub mod mmap;

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
        eprintln!(
            "[{:?}] {}",
            $crate::trace::elapsed(),
            format_args!($($arg)*)
        );
    };
}

#[cfg(not(feature = "trace"))]
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {};
}
