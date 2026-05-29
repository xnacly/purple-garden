//! utilities used by multiple purple-garden crates.

#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
compile_error!("purple-garden-shared currently supports only Linux on x86_64 or aarch64");

pub mod config;
pub mod mmap;
