use crate::{ast::Node, err::PgError, lex::Lexer};

pub struct Parser<'p> {
    lex: Lexer<'p>,
}

impl<'p> Parser<'p> {
    pub fn new(lex: Lexer<'p>) -> Self {
        Self { lex }
    }

    pub fn parse(self) -> Result<Vec<Node<'p>>, PgError> {
        Ok(vec![])
    }
}
