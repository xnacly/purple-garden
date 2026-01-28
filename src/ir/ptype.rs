use crate::ir::Const;

/// Compile time type system,
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Type {
    Bool,
    Int,
    Double,
    Str,
    Option(Box<Type>),
    Array(Box<Type>),
    Map(Box<Type>),
}

impl From<Const<'_>> for Type {
    fn from(value: Const<'_>) -> Self {
        match value {
            Const::True | Const::False => Self::Bool,
            Const::Int(_) => Self::Int,
            Const::Double(_) => Self::Double,
            Const::Str(_) => Self::Str,
        }
    }
}
