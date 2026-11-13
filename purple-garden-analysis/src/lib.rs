//! Frontend analysis of a single document: diagnostics, hovers, definitions and completions. Shared
//! by the language server and the visualizer's editor.

mod analysis;
mod code_action;
mod collect;
mod completion;
mod definition;
mod diagnostic;
pub mod doc;
pub mod frontend;
mod hover;
mod source;

pub use analysis::DocumentState;
pub use source::{apply_content_changes, offset_for_position, position_for_offset, range_for_span};
