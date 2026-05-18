//! IR-level optimisation passes.
//!
//! Each pass lives in its own submodule. Orchestration (which passes
//! run, in what order) lives in [crate::opt::ir] in `src/opt/mod.rs`.

mod indirect_jump;
mod ret_inline;
mod tailcall;

pub use indirect_jump::indirect_jump;
pub use ret_inline::ret_inline;
pub use tailcall::tailcall;
