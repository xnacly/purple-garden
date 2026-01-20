use crate::{
    ast::Node,
    err::PgError,
    lex::{Lexer, Token, Type},
};

pub struct Parser<'p> {
    lex: &'p mut Lexer<'p>,
    cur: Token<'p>,
}

impl<'p> Parser<'p> {
    pub fn new(lex: &'p mut Lexer<'p>) -> Result<Self, PgError> {
        let cur = lex.next()?;
        Ok(Self { cur, lex })
    }

    fn cur(&self) -> &Token<'p> {
        &self.cur
    }

    fn at_end(&self) -> bool {
        self.cur.t == Type::Eof
    }

    fn expect(&mut self, ty: Type) -> Result<(), PgError> {
        if self.cur.t == ty {
            self.next()
        } else {
            Err(PgError::with_msg(
                format!("Expected `{:?}`, got {:?}", ty, self.cur.t),
                &self.cur,
            ))
        }
    }

    fn next(&mut self) -> Result<(), PgError> {
        self.cur = self.lex.next()?;
        Ok(())
    }

    pub fn parse(mut self) -> Result<Vec<Node<'p>>, PgError> {
        let mut raindrain = vec![];
        while !self.at_end() {
            raindrain.push(self.parse_prefix()?);
        }
        Ok(raindrain)
    }

    fn parse_prefix(&mut self) -> Result<Node<'p>, PgError> {
        match self.cur().t {
            Type::Let => self.parse_let(),
            Type::Fn => self.parse_fn(),
            Type::Match => self.parse_match(),
            Type::For => self.parse_for(),
            Type::Ident(_) => {
                let i = Node::Ident {
                    name: self.cur.clone(),
                };
                self.next()?;
                Ok(i)
            }
            Type::String(_) | Type::Integer(_) | Type::Double(_) | Type::True | Type::False => {
                self.parse_atom()
            }
            _ => self.parse_expr(),
        }
    }

    fn parse_let(&mut self) -> Result<Node<'p>, PgError> {
        self.expect(Type::Let)?;
        let cur = self.cur.clone();
        let Type::Ident(name) = cur.t else {
            return Err(PgError::with_msg(
                "Wanted an ident for the lhs of a let stmt",
                &self.cur,
            ));
        };

        self.next()?;
        self.expect(Type::Equal)?;
        let rhs = Box::new(self.parse_prefix()?);
        Ok(Node::Let { name: cur, rhs })
    }

    fn parse_fn(&mut self) -> Result<Node<'p>, PgError> {
        todo!();
    }

    fn parse_match(&mut self) -> Result<Node<'p>, PgError> {
        todo!();
    }

    fn parse_for(&mut self) -> Result<Node<'p>, PgError> {
        todo!();
    }

    fn parse_expr(&mut self) -> Result<Node<'p>, PgError> {
        todo!();
    }

    fn parse_atom(&mut self) -> Result<Node<'p>, PgError> {
        let atom = match self.cur.t {
            Type::String(_) | Type::Integer(_) | Type::Double(_) | Type::True | Type::False => {
                Node::Atom {
                    raw: self.cur.clone(),
                }
            }
            _ => todo!("Parser::parse_atom$pratt_parsing"),
        };
        self.next()?;
        Ok(atom)
    }
}

/// Happy path ofc :^)
#[cfg(test)]
mod happy {
    use crate::{
        ast::Node,
        lex::{Lexer, Token, Type},
        parser::Parser,
    };

    #[test]
    fn parse_let() {
        let input = "let variable_name = 5";
        let mut l = Lexer::new(input.as_bytes());
        let p = Parser::new(&mut l).unwrap();
        let ast = p.parse().unwrap();

        assert_eq!(
            ast,
            vec![Node::Let {
                name: Token {
                    line: 0,
                    col: "let variable_name".len(),
                    t: Type::Ident("variable_name")
                },
                rhs: Box::new(Node::Atom {
                    raw: Token {
                        line: 0,
                        col: "let variable_name = 5".len(),
                        t: Type::Integer("5"),
                    },
                })
            }]
        );
    }
}
