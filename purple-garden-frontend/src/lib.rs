#![feature(portable_simd)]

pub mod ast;
pub mod err;
pub mod lex;
pub mod lower;
pub mod parser;
pub mod typecheck;

use ast::TypeExpr;
use purple_garden_ir::ptype;

#[must_use]
pub fn type_from_atom_token_type(t: &lex::Type<'_>) -> ptype::Type {
    match t {
        lex::Type::S(_) => ptype::Type::Str,
        lex::Type::D(_) => ptype::Type::Double,
        lex::Type::I(_) => ptype::Type::Int,
        lex::Type::True | lex::Type::False => ptype::Type::Bool,
        _ => unreachable!("{:?}", t),
    }
}

#[must_use]
pub fn type_from_lex_type(t: lex::Type<'_>) -> ptype::Type {
    match t {
        lex::Type::Int => ptype::Type::Int,
        lex::Type::Double => ptype::Type::Double,
        lex::Type::Str => ptype::Type::Str,
        lex::Type::Bool => ptype::Type::Bool,
        lex::Type::Void => ptype::Type::Void,
        _ => unreachable!(),
    }
}

#[must_use]
pub fn type_from_type_expr(value: &TypeExpr<'_>) -> ptype::Type {
    match value {
        TypeExpr::Atom(token) => type_from_lex_type(token.t),
        TypeExpr::Option(type_expr) => {
            ptype::Type::Option(Box::new(type_from_type_expr(type_expr.as_ref())))
        }
        TypeExpr::Array(type_expr) => {
            ptype::Type::Array(Box::new(type_from_type_expr(type_expr.as_ref())))
        }
    }
}
