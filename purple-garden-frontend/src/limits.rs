//! Limits on the shape of a program. The parser and the typechecker reject a program exceeding one
//! with a diagnostic, so no later stage has to handle it: every pass recursing over the syntax tree
//! is bounded by [`MAX_NESTING`], and every pass recursing over a type by [`MAX_TYPE_PARTS`].

use crate::diagnostic::{Diagnostic, Help, Span};

/// How deep syntax nests: expressions, blocks, declarations and type expressions, counting every
/// operand of a left-associative chain such as `a + b + c` or `x.a.b` as one more level, since
/// the tree it builds is that deep.
///
/// Every stage walking the tree recurses once per level, so this bounds the native stack they
/// use: a program nested this deep compiles on a 2 MiB thread in a release build.
pub const MAX_NESTING: usize = 256;

/// Parts of a type, see [`purple_garden_ir::ptype::Type::parts_exceed`]: `Record<a: Int b:
/// Array<Str>>` has four. Bounds how deep a type nests, how long it takes to compare or print, and
/// the size of a record (records are stored inline), so its field offsets fit the bytecode's.
///
/// Types also grow without nesting syntax, `let b = { x: a y: a }` doubles the parts of `a`.
pub const MAX_TYPE_PARTS: usize = 256;

/// Diagnostics one stage (lexing and parsing, typechecking) reports. Recovery can report one per
/// token of a broken file, and every one costs a look at its source line to show it, so past this
/// many they are dropped and the parser stops.
pub const MAX_DIAGNOSTICS: usize = 100;

/// Add `diagnostic` to `diagnostics` unless they already hold [`MAX_DIAGNOSTICS`]; the last one
/// kept says so.
pub fn report(diagnostics: &mut Vec<Diagnostic>, diagnostic: Diagnostic) {
    match diagnostics.len().cmp(&(MAX_DIAGNOSTICS - 1)) {
        std::cmp::Ordering::Less => diagnostics.push(diagnostic),
        std::cmp::Ordering::Equal => diagnostics.push(
            diagnostic.with_note(format!("stopped after {MAX_DIAGNOSTICS} errors")),
        ),
        std::cmp::Ordering::Greater => {}
    }
}

#[must_use]
pub fn nested_too_deep(at: Span) -> Diagnostic {
    Diagnostic::new(format!("Nested deeper than {MAX_NESTING} levels"), at).with_help(Help::new(
        "move inner parts into `let` bindings or functions",
    ))
}

#[must_use]
pub fn type_too_large(at: Span) -> Diagnostic {
    Diagnostic::new(
        format!("Type has more than {MAX_TYPE_PARTS} parts"),
        at,
    )
    .with_note("every type and every type a record, array or option holds counts as a part")
    .with_help(Help::new("split it into smaller records"))
}
