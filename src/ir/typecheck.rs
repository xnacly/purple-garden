use std::collections::HashMap;

use crate::{err::PgError, ir::ptype};

/// walks the Ast, performs type checking and assigns each node a type by its id
fn typecheck() -> Result<HashMap<usize, ptype::Type>, PgError> {
    todo!()
}
