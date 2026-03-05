#![cfg_attr(feature = "nightly", feature(likely_unlikely))]
#![allow(unused)]

pub mod asm;
pub mod ast;
pub mod bc;
pub mod config;
pub mod err;
pub mod gc;
pub mod ir;
pub mod jit;
pub mod lex;
pub mod opt;
pub mod parser;
pub mod vm;

#[cfg(feature = "trace")]
pub mod trace {
    use super::*;
    use std::sync::Once;
    use std::time::Instant;

    static START_ONCE: Once = Once::new();
    static mut START: Option<Instant> = None;

    pub fn start() -> Instant {
        unsafe {
            START_ONCE.call_once(|| {
                START = Some(Instant::now());
            });
            START.unwrap()
        }
    }

    #[macro_export]
    macro_rules! trace {
        // With values
        ($fmt:literal, $($value:expr),*) => {
            {
                let elapsed = $crate::trace::start().elapsed();
                println!("[{:?}] {}", elapsed, format_args!($fmt, $($value),*));
            }
        };
        // Without values
        ($fmt:literal) => {
            {
                let elapsed = $crate::trace::start().elapsed();
                println!("[{:?}] {}", elapsed, $fmt);
            }
        };
    }
}

#[cfg(not(feature = "trace"))]
pub mod trace {
    #[macro_export]
    macro_rules! trace {
        ($fmt:literal, $($value:expr),*) => {};
        ($fmt:literal) => {};
    }
}
