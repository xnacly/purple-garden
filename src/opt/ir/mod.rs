//! IR-level optimisation passes.
//!
//! Each pass lives in its own submodule. Orchestration (which passes
//! run, in what order) lives in [crate::opt::ir] in `src/opt/mod.rs`.

mod imm_fold;
mod indirect_jump;
mod ret_inline;
mod tailcall;

pub use imm_fold::Scratch as ImmFoldScratch;
pub use imm_fold::imm_fold;
pub use indirect_jump::indirect_jump;
pub use ret_inline::ret_inline;
pub use tailcall::tailcall;
