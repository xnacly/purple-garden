use std::collections::HashMap;

use crate::{ast::Node, err::PgError, ir::*};

#[derive(Default)]
pub struct Lower<'lower> {
    functions: Vec<Func>,
    current_func: Option<Func>,
    current_block: Option<Id>,
    /// maps ast variable names to ssa values
    env: HashMap<&'lower str, Id>,
}

impl Lower<'_> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lower [ast] into a list of Func nodes, the entry point is always `__pg_entry`
    pub fn ir_from(&mut self, ast: &[Node]) -> Result<Vec<Func>, PgError> {
        dbg!(ast);
        Ok(vec![])
    }
}

#[cfg(test)]
mod lower {
    #[test]
    fn atom() {}
    #[test]
    fn ident() {}
    #[test]
    fn bin() {}
    #[test]
    fn r#let() {}
    #[test]
    fn r#fn() {}
    #[test]
    fn r#call() {}
    #[test]
    fn r#match() {}
    #[test]
    fn path() {}
}
