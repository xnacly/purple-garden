//! Purple garden type system
use std::fmt::Display;

use crate::{ast::TypeExpr, ir::Const, lex};

/// Compile time type system,
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Type {
    Bool,
    Int,
    Double,
    Str,
    Option(Box<Type>),
    Array(Box<Type>),
    Map { key: Box<Type>, value: Box<Type> },
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Bool => write!(f, "Bool"),
            Type::Int => write!(f, "Int"),
            Type::Double => write!(f, "Double"),
            Type::Str => write!(f, "Str"),
            Type::Option(inner) => write!(f, "Option<{}>", inner),
            Type::Array(inner) => write!(f, "Array<{}>", inner),
            Type::Map { key, value } => {
                write!(f, "Map<{}, {}>", key, value)
            }
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
        }
    }
}

impl From<lex::Type<'_>> for Type {
    fn from(value: lex::Type) -> Self {
        match value {
            lex::Type::Int => Self::Int,
            lex::Type::Double => Self::Double,
            lex::Type::Str => Self::Str,
            lex::Type::Bool => Self::Bool,
            _ => unreachable!(),
        }
    }
}

impl From<&TypeExpr<'_>> for Type {
    fn from(value: &TypeExpr<'_>) -> Self {
        match value {
            TypeExpr::Atom(token) => token.t.into(),
            TypeExpr::Option(type_expr) => Type::Option(Box::new(type_expr.as_ref().into())),
            TypeExpr::Array(type_expr) => Type::Array(Box::new(type_expr.as_ref().into())),
            TypeExpr::Map { key, value } => Type::Map {
                key: Box::new(key.as_ref().into()),
                value: Box::new(value.as_ref().into()),
            },
        }
    }
}
