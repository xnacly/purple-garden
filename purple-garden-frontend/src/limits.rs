//! Limits on the shape of a program. The parser and the typechecker reject a program exceeding one
//! with a diagnostic, so no later stage has to handle it: every pass recursing over the syntax tree
//! is bounded by [`MAX_NESTING`].

use crate::diagnostic::{Diagnostic, Help, Span};

/// How deep syntax nests: expressions, blocks, declarations and type expressions, counting every
/// operand of a left-associative chain such as `a + b + c` or `x.a.b` as one more level, since
/// the tree it builds is that deep.
///
/// Every stage walking the tree recurses once per level, so this bounds the native stack they
/// use: a program nested this deep compiles on a 2 MiB thread in a release build.
pub const MAX_NESTING: usize = 256;

#[must_use]
pub fn nested_too_deep(at: Span) -> Diagnostic {
    Diagnostic::new(format!("Nested deeper than {MAX_NESTING} levels"), at).with_help(Help::new(
        "move inner parts into `let` bindings or functions",
    ))
}
