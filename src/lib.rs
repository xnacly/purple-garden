#![cfg_attr(feature = "nightly", feature(likely_unlikely))]
#![allow(unused)]

use crate::{
    config::Config,
    err::PgError,
    vm::{Value, Vm},
};

pub mod asm;
pub mod ast;
pub mod bc;
pub mod config;
pub mod err;
pub mod gc;
pub mod help;
pub mod input;
pub mod ir;
pub mod jit;
pub mod lex;
pub mod mmap;
pub mod opt;
pub mod parser;
pub mod std;
pub mod vm;

/// Create the purple garden vm from the given input.
pub fn new<'e>(config: &'e config::Config, input: &'e [u8]) -> Result<Vm<'e>, PgError> {
    let lexer = lex::Lexer::new(input);
    let ast = parser::Parser::new(lexer)?.parse()?;

    let mut ir = ir::lower::Lower::new().ir_from(&ast)?;
    if config.opt >= 1 {
        opt::ir(&mut ir);
    }

    let mut cc = bc::Cc::new();
    cc.compile(&config, &ir)?;
    if config.opt >= 1 {
        opt::bc(&mut cc.buf);
    }

    Ok(cc.finalize(config))
}

#[cfg(feature = "trace")]
pub mod trace {
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
