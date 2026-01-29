use crate::{ast::Node, err::PgError, ir::ptype};

struct TypedNode<'n> {
    node: &'n Node<'n>,
    ty: ptype::Type,
}

/// walks the Ast, performs type checking and wraps each node with a TypedNode
fn typecheck() -> Result<(), PgError> {
    todo!()
}
