use crate::{
    ast::{Node, TypeExpr},
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
            self.next()?;
        } else {
            return Err(PgError::with_msg(
                format!("Expected `{:?}`, got {:?}", ty, self.cur.t),
                &self.cur,
            ));
        }

        Ok(())
    }

    fn expect_ident(&mut self) -> Result<Token<'p>, PgError> {
        if let Type::Ident(_) = self.cur.t {
            let matched = self.cur.clone();
            self.next()?;
            Ok(matched)
        } else {
            Err(PgError::with_msg(
                format!("Expected an identifier, got {:?}", self.cur.t),
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

    /// `let <name> = <rhs>`
    fn parse_let(&mut self) -> Result<Node<'p>, PgError> {
        self.expect(Type::Let)?;
        let name = self.expect_ident()?;
        self.expect(Type::Equal)?;

        Ok(Node::Let {
            name,
            rhs: Box::new(self.parse_prefix()?),
        })
    }

    /// fn <name>(<arg0:type0> <arg1:type1>): <return_type> {
    ///     <body>
    /// }
    fn parse_fn(&mut self) -> Result<Node<'p>, PgError> {
        todo!("Parser::parse_fn");
    }

    fn parse_match(&mut self) -> Result<Node<'p>, PgError> {
        todo!("Parser::parse_match");
    }

    fn parse_for(&mut self) -> Result<Node<'p>, PgError> {
        todo!("Parser::parse_for");
    }

    fn parse_expr(&mut self) -> Result<Node<'p>, PgError> {
        todo!("Parser::expr");
    }

    fn parse_atom(&mut self) -> Result<Node<'p>, PgError> {
        let atom_or_wrapped_expression = match self.cur.t {
            Type::String(_) | Type::Integer(_) | Type::Double(_) | Type::True | Type::False => {
                Node::Atom {
                    raw: self.cur.clone(),
                }
            }
            Type::BraceLeft => {
                self.next()?;
                let e = self.parse_expr()?;
                self.expect(Type::BraceRight)?;
                e
            }
            _ => {
                return Err(PgError::with_msg(
                    "Expected atom or expr wrapped by ()",
                    self.cur(),
                ));
            }
        };
        self.next()?;
        Ok(atom_or_wrapped_expression)
    }

    fn parse_type(&mut self) -> Result<TypeExpr<'p>, PgError> {
        let Token { t, .. } = self.cur();
        Ok(match t {
            // Optionals: ?<type>
            Type::Question => {
                self.next()?;
                TypeExpr::Option(Box::new(self.parse_type()?))
            }
            // Arrays: [<type>]
            Type::BraketLeft => {
                self.next()?;
                let inner = Box::new(self.parse_type()?);
                self.expect(Type::BraketRight)?;
                TypeExpr::Array(inner)
            }
            // Atom types
            Type::TStr | Type::TInt | Type::TBool | Type::TVoid | Type::TDouble => {
                let tt = TypeExpr::Atom(self.cur().clone());
                self.next()?;

                // Map/object <type>[<type>]
                if self.cur().t == Type::BraketLeft {
                    self.next()?;
                    let value = self.parse_type()?;
                    self.expect(Type::BraketRight)?;
                    TypeExpr::Map {
                        key: Box::new(tt),
                        value: Box::new(value),
                    }
                } else {
                    tt
                }
            }
            _ => {
                return Err(PgError::with_msg(
                    "Bad type, expected either type, ?type, [type] or type[type], where type is str, int, double, bool or void",
                    self.cur(),
                ));
            }
        })
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
