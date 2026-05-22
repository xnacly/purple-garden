//! Peephole passes operating on the linearised bytecode buffer.
//!
//! Each pass lives in its own submodule so the file stays scannable as
//! we add more patterns. Orchestration (window sizing, pass ordering)
//! lives in [crate::opt::bc] in `src/opt/mod.rs`.

mod jmp_next;
mod mov_merge;
mod self_move;

pub use jmp_next::jmp_next;
pub use mov_merge::mov_merge;
pub use self_move::self_move;
