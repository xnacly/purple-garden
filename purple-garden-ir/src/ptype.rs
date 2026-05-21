//! Purple garden type system
use std::fmt::Display;

use crate::Const;

/// Compile time type system,
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum Type {
    Void,
    Bool,
    Int,
    Double,
    Str,
    Option(Box<Type>),
    Array(Box<Type>),
    // Foreign type for handling opaque rust data feed into the vm runtime
    //
    // which is useful for something like Foreign("counter") vs
    // Foreign("player") in the typesystem, meaning functions defined on the former can not be
    // called on the latter, resulting in a type error
    Foreign(&'static str),
    // TODO: add Record type for structuring data together as fields
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Void => write!(f, "Void"),
            Type::Bool => write!(f, "Bool"),
            Type::Int => write!(f, "Int"),
            Type::Double => write!(f, "Double"),
            Type::Str => write!(f, "Str"),
            Type::Foreign(id) => write!(f, "Foreign<{id}>"),
            Type::Option(inner) => write!(f, "Option<{}>", inner),
            Type::Array(inner) => write!(f, "Array<{}>", inner),
        }
    }
}

impl From<Const<'_>> for Type {
    fn from(value: Const<'_>) -> Self {
        match value {
            Const::True | Const::False => Self::Bool,
            Const::Int(_) => Self::Int,
            Const::Double(_) => Self::Double,
            Const::Str(_) => Self::Str,
            _ => unreachable!(),
        }
    }
}
