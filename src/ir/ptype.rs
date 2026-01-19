/// Compile time type system,
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Type {
    Bool,
    Int,
    Double,
    Str,
}
