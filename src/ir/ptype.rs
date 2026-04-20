//! Purple garden type system
use std::fmt::Display;

use crate::{ast::TypeExpr, ir::Const, lex};

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

impl Type {
    pub fn from_atom_token_type(t: &lex::Type) -> Self {
        match t {
            lex::Type::S(_) => Self::Str,
            lex::Type::D(_) => Self::Double,
            lex::Type::I(_) => Self::Int,
            lex::Type::True | lex::Type::False => Self::Bool,
            _ => unreachable!("{:?}", t),
        }
    }
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

impl From<lex::Type<'_>> for Type {
    fn from(value: lex::Type) -> Self {
        match value {
            lex::Type::Int => Self::Int,
            lex::Type::Double => Self::Double,
            lex::Type::Str => Self::Str,
            lex::Type::Bool => Self::Bool,
            lex::Type::Void => Self::Void,
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
        }
    }
}
