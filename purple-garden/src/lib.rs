use purple_garden_bc as bc;
use purple_garden_frontend::{err::PgError, lex, lower, parser};
use purple_garden_runtime::{Vm, VmConfig};

pub mod config;
pub mod gc;
pub mod help;
pub mod input;
pub mod jit;

pub use purple_garden_shared::mmap;

/// Create the purple garden vm from the given input. Returns the Vm
/// alongside the compile-time `DebugInfo` needed to render runtime
/// traps with `file:line:col`. Callers that don't care about
/// diagnostics (benches, internal tests) can ignore the second tuple
/// element.
pub fn new<'e>(
    config: &'e config::Config,
    input: &'e [u8],
) -> Result<(Vm, bc::DebugInfo), PgError> {
    let lexer = lex::Lexer::new(input);
    let ast = parser::Parser::new(lexer)?.parse()?;

    let mut ir = lower::Lower::new().ir_from(&ast)?;
    if config.opt >= 1 {
        purple_garden_opt::ir(&mut ir);
    }

    let mut cc = bc::Cc::new();
    cc.compile(config.liveness, &ir);
    if config.opt >= 1 {
        purple_garden_opt::bc(&mut cc.buf);
        cc.compact_nops();
    }

    Ok(cc.finalize(VmConfig {
        backtrace: config.backtrace,
    }))
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
