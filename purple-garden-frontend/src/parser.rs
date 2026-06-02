use crate::{
    ast::{Node, TypeExpr},
    err::PgError,
    lex::{Lexer, Token, Type},
};

// TODO: add BNAF to each Parser::parse_* method

/// Parsing the token stream one token at a time into the abstract syntax tree, see
/// [ast.rs](./ast.rs) for documentation regarding each node and the way those should be parsed.
pub struct Parser<'p> {
    lex: Lexer<'p>,
    id: usize,
    cur: Token<'p>,
}

impl<'p> Parser<'p> {
    pub fn new(mut lex: Lexer<'p>) -> Result<Self, PgError> {
        let cur = lex.one()?;
        Ok(Self { cur, lex, id: 0 })
    }

    fn next_id(&mut self) -> usize {
        let id = self.id;
        self.id += 1;
        id
    }

    fn cur(&self) -> &Token<'p> {
        &self.cur
    }

    fn at_end(&self) -> bool {
        self.cur.t == Type::Eof
    }

    fn expect(&mut self, ty: Type) -> Result<(), PgError> {
        if self.cur.t == ty {
            self.advance()?;
        } else {
            return Err(PgError::with_msg(
                format!(
                    "Expected `{:?}`, got {}({:?})",
                    ty,
                    self.cur.t.as_str(),
                    self.cur.t
                ),
                &self.cur,
            ));
        }

        Ok(())
    }

    fn expect_ident(&mut self) -> Result<Token<'p>, PgError> {
        if let Type::Ident(_) = self.cur.t {
            let matched = self.cur.clone();
            self.advance()?;
            Ok(matched)
        } else {
            Err(PgError::with_msg(
                format!("Expected an identifier, got {:?}", self.cur.t),
                &self.cur,
            ))
        }
    }

    fn advance(&mut self) -> Result<(), PgError> {
        self.cur = self.lex.one()?;
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
            Type::Import => self.parse_import(),
            Type::Let => self.parse_let(),
            Type::Fn => self.parse_fn(),
            Type::Match => self.parse_match(),
            _ => self.parse_expr(0),
        }
    }

    fn parse_import(&mut self) -> Result<Node<'p>, PgError> {
        let src = self.cur.clone();
        // skip Type::Import
        self.advance()?;

        // single package import:
        // import "io"
        if let Type::S(_) = self.cur().t {
            let pkgs = vec![self.cur().clone()];
            // skip pkg name
            self.advance()?;

            return Ok(Node::Import {
                src,
                id: self.next_id(),
                pkgs,
            });
        }

        // multiple package import:
        // import ("io" "runtime")

        self.expect(Type::BraceLeft)?;

        let mut pkgs = Vec::new();

        while !self.at_end() && self.cur().t != Type::BraceRight {
            let &Token { t: Type::S(_), .. } = self.cur() else {
                return Err(PgError::with_msg(
                    "Only strings are allowed as import paths",
                    &self.cur,
                ));
            };
            pkgs.push(self.cur().clone());
            self.advance()?;
        }

        self.expect(Type::BraceRight)?;
        Ok(Node::Import {
            src,
            id: self.next_id(),
            pkgs,
        })
    }

    fn parse_let(&mut self) -> Result<Node<'p>, PgError> {
        self.expect(Type::Let)?;
        let name = self.expect_ident()?;
        self.expect(Type::Equal)?;

        Ok(Node::Let {
            id: self.next_id(),
            name,
            rhs: Box::new(self.parse_prefix()?),
        })
    }

    fn parse_fn(&mut self) -> Result<Node<'p>, PgError> {
        self.advance()?;
        let name = self.expect_ident()?;

        self.expect(Type::BraceLeft)?;
        let mut args = vec![];
        while !self.at_end() && self.cur().t != Type::BraceRight {
            let arg_name = self.expect_ident()?;
            self.expect(Type::Colon)?;
            let arg_type = self.parse_type()?;
            args.push((arg_name, arg_type));
        }
        self.expect(Type::BraceRight)?;

        let return_type = if self.cur().t == Type::CurlyLeft {
            TypeExpr::Atom(Token {
                start: self.cur().start,
                t: Type::Void,
            })
        } else {
            self.parse_type()?
        };

        let mut body = vec![];
        self.expect(Type::CurlyLeft)?;
        while !self.at_end() && self.cur().t != Type::CurlyRight {
            body.push(self.parse_prefix()?);
        }
        self.expect(Type::CurlyRight)?;

        Ok(Node::Fn {
            name,
            args,
            return_type,
            body,
        })
    }

    fn parse_match(&mut self) -> Result<Node<'p>, PgError> {
        self.advance()?;
        let mut cases = vec![];
        let mut default = None;
        let tok = self.cur().clone();

        self.expect(Type::CurlyLeft)?;
        while !self.at_end() && self.cur().t != Type::CurlyRight {
            // default case
            if self.cur().t == Type::CurlyLeft {
                let default_token = self.cur().clone();
                self.expect(Type::CurlyLeft)?;
                let mut default_body = vec![];
                while !self.at_end() && self.cur().t != Type::CurlyRight {
                    default_body.push(self.parse_prefix()?);
                }
                self.expect(Type::CurlyRight)?;
                default = Some((default_token, default_body));
            } else {
                let condition_token = self.cur().clone();
                let condition = self.parse_expr(0)?;
                self.expect(Type::CurlyLeft)?;
                let mut body = vec![];
                while !self.at_end() && self.cur().t != Type::CurlyRight {
                    body.push(self.parse_prefix()?);
                }
                self.expect(Type::CurlyRight)?;
                cases.push(((condition_token, condition), body));
            }
        }
        self.expect(Type::CurlyRight)?;

        let Some(default) = default else {
            return Err(PgError::with_msg(
                "A match statement requires a default branch",
                &tok,
            ));
        };

        Ok(Node::Match {
            id: self.next_id(),
            cases,
            default,
        })
    }

    fn parse_expr(&mut self, min_bp: u8) -> Result<Node<'p>, PgError> {
        let mut lhs = match self.cur().t {
            Type::S(_) | Type::I(_) | Type::D(_) | Type::True | Type::False => {
                let raw = self.cur.clone();
                self.advance()?;
                Node::Atom {
                    raw,
                    id: self.next_id(),
                }
            }
            Type::Ident(_) => {
                let first = self.cur.clone();
                self.advance()?;

                Node::Ident {
                    name: first,
                    id: self.next_id(),
                }
            }
            Type::BraceLeft => {
                self.advance()?;
                let e = self.parse_expr(0)?;
                self.expect(Type::BraceRight)?;
                e
            }
            Type::Plus | Type::Minus => {
                let op = self.cur().clone();
                let rbp = Parser::prefix_binding_power(&self.cur().t);
                self.advance()?;
                let rhs = self.parse_expr(rbp)?;
                Node::Unary {
                    id: self.next_id(),
                    op,
                    rhs: Box::new(rhs),
                }
            }
            _ => todo!("{:?}", self.cur().t),
        };

        // postfix parsing loop
        loop {
            match self.cur().t {
                Type::Dot => {
                    self.advance()?;
                    let field = self.expect_ident()?;

                    lhs = Node::Field {
                        id: self.next_id(),
                        target: Box::new(lhs),
                        name: field,
                    };
                }

                Type::BraceLeft => {
                    if !Self::is_callable_target(&lhs) {
                        break;
                    }

                    self.advance()?;
                    let mut args = vec![];

                    while !self.at_end() && self.cur().t != Type::BraceRight {
                        args.push(self.parse_prefix()?);
                    }

                    self.expect(Type::BraceRight)?;
                    lhs = Node::Call {
                        id: self.next_id(),
                        target: Box::new(lhs),
                        args,
                    }
                }
                _ => break,
            }
        }

        // infix parsing loop
        while let Type::Plus
        | Type::Minus
        | Type::Asteriks
        | Type::Slash
        | Type::Percent
        | Type::DoubleEqual
        | Type::As
        | Type::LessThan
        | Type::GreaterThan = self.cur().t
        {
            let op = self.cur().clone();

            if let Token { t: Type::As, .. } = op {
                self.advance()?;
                let ty = self.parse_type()?;
                lhs = Node::Cast {
                    src: op,
                    id: self.next_id(),
                    lhs: Box::new(lhs),
                    rhs: ty,
                };
                continue;
            }

            if let Some((lbp, rbp)) = Parser::infix_binding_power(&op.t) {
                if lbp < min_bp {
                    break;
                }

                self.advance()?;

                let rhs = self.parse_expr(rbp)?;
                lhs = Node::Bin {
                    id: self.next_id(),
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            }
        }

        Ok(lhs)
    }

    fn is_callable_target(node: &Node<'p>) -> bool {
        matches!(node, Node::Ident { .. } | Node::Field { .. })
    }

    fn prefix_binding_power(op: &Type) -> u8 {
        // TODO: add !<expr>,?<expr>, the latter being the try operator
        match op {
            Type::Plus | Type::Minus => 5,
            _ => unreachable!(),
        }
    }

    fn infix_binding_power(op: &Type) -> Option<(u8, u8)> {
        Some(match op {
            Type::Plus | Type::Minus => (1, 2),
            Type::Asteriks | Type::Slash | Type::Percent => (3, 4),
            Type::DoubleEqual | Type::NotEqual | Type::GreaterThan | Type::LessThan => (0, 1),
            _ => return None,
        })
    }

    fn parse_type(&mut self) -> Result<TypeExpr<'p>, PgError> {
        let Token { t, .. } = self.cur();
        Ok(match t {
            // Atom types
            Type::Str | Type::Int | Type::Bool | Type::Void | Type::Double => {
                let tt = TypeExpr::Atom(self.cur().clone());
                self.advance()?;
                tt
            }
            // Foreign types: Foreign<Counter>
            Type::Ident("Foreign") => {
                self.advance()?;
                self.expect(Type::LessThan)?;
                if self.cur().t == Type::GreaterThan {
                    return Err(PgError::with_msg(
                        "Expected a foreign type name after `Foreign<`",
                        &self.cur,
                    ));
                }
                let inner = self.expect_ident()?;
                self.expect(Type::GreaterThan)?;
                TypeExpr::Foreign(inner)
            }
            // Optionals: Option<type>
            Type::Ident("Option") => {
                self.advance()?;
                self.expect(Type::LessThan)?;
                if self.cur().t == Type::GreaterThan {
                    return Err(PgError::with_msg(
                        "Expected a type after `Option<`",
                        &self.cur,
                    ));
                }
                let inner = Box::new(self.parse_type()?);
                self.expect(Type::GreaterThan)?;
                TypeExpr::Option(inner)
            }
            // Arrays: Array<type>
            Type::Ident("Array") => {
                self.advance()?;
                self.expect(Type::LessThan)?;
                if self.cur().t == Type::GreaterThan {
                    return Err(PgError::with_msg(
                        "Expected a type after `Array<`",
                        &self.cur,
                    ));
                }
                let inner = Box::new(self.parse_type()?);
                self.expect(Type::GreaterThan)?;
                TypeExpr::Array(inner)
            }
            _ => {
                return Err(PgError::with_msg(
                    "Bad type, expected one of: Str, Int, Double, Bool, Void, Foreign<name>, Option<type> or Array<type>",
                    self.cur(),
                ));
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ast::{Node, TypeExpr},
        lex::{Lexer, Token, Type},
        parser::Parser,
    };

    macro_rules! mk_tok {
        ($type:expr) => {
            Token { start: 0, t: $type }
        };
    }

    macro_rules! table_parse_types {
        ($group:ident,$(($name:ident,$input:literal,$expected:expr))*) => {
            mod $group {
                use super::*;

                $(
                    #[test]
                    fn $name() {
                        let l = Lexer::new($input.as_bytes());
                        let mut p = Parser::new(l).unwrap();
                        let tt = p.parse_type().unwrap();
                        assert_eq!(tt, $expected);
                    }
                )*
            }
        };
    }

    table_parse_types! {
        parse_types_atom,
        (int,"Int",TypeExpr::Atom(mk_tok!(Type::Int)))
        (double,"Double", TypeExpr::Atom(mk_tok!(Type::Double)))
        (str,"Str", TypeExpr::Atom(mk_tok!(Type::Str)))
        (bool,"Bool", TypeExpr::Atom(mk_tok!(Type::Bool)))
        (void,"Void", TypeExpr::Atom(mk_tok!(Type::Void)))
    }

    table_parse_types! {
        parse_types_option,
        (int,"Option<Int>", TypeExpr::Option(Box::new(TypeExpr::Atom(mk_tok!(Type::Int)))))
        (double,"Option<Double>", TypeExpr::Option(Box::new(TypeExpr::Atom(mk_tok!(Type::Double)))))
        (str,"Option<Str>", TypeExpr::Option(Box::new(TypeExpr::Atom(mk_tok!(Type::Str)))))
        (bool,"Option<Bool>", TypeExpr::Option(Box::new(TypeExpr::Atom(mk_tok!(Type::Bool)))))
        (void,"Option<Void>", TypeExpr::Option(Box::new(TypeExpr::Atom(mk_tok!(Type::Void)))))
        (double_wrapped,"Option<Option<Void>>", TypeExpr::Option(Box::new(TypeExpr::Option(Box::new(TypeExpr::Atom(mk_tok!(Type::Void)))))))
    }

    table_parse_types! {
        parse_types_array,
        (int,"Array<Int>", TypeExpr::Array(Box::new(TypeExpr::Atom(mk_tok!(Type::Int)))))
        (double,"Array<Double>", TypeExpr::Array(Box::new(TypeExpr::Atom(mk_tok!(Type::Double)))))
        (str,"Array<Str>", TypeExpr::Array(Box::new(TypeExpr::Atom(mk_tok!(Type::Str)))))
        (bool,"Array<Bool>", TypeExpr::Array(Box::new(TypeExpr::Atom(mk_tok!(Type::Bool)))))
        (void,"Array<Void>", TypeExpr::Array(Box::new(TypeExpr::Atom(mk_tok!(Type::Void)))))
        (double_wrapped,"Array<Array<Void>>", TypeExpr::Array(Box::new(TypeExpr::Array(Box::new(TypeExpr::Atom(mk_tok!(Type::Void)))))))
    }

    table_parse_types! {
        parse_types_foreign,
        (
            counter,
            "Foreign<Counter>",
            TypeExpr::Foreign(mk_tok!(Type::Ident("Counter")))
        )
    }

    #[test]
    fn empty_foreign_has_specific_error() {
        let l = Lexer::new(b"Foreign<>");
        let mut p = Parser::new(l).unwrap();
        let err = p.parse_type().unwrap_err();
        assert_eq!(err.msg, "Expected a foreign type name after `Foreign<`");
    }

    #[test]
    fn empty_option_has_specific_error() {
        let l = Lexer::new(b"Option<>");
        let mut p = Parser::new(l).unwrap();
        let err = p.parse_type().unwrap_err();
        assert_eq!(err.msg, "Expected a type after `Option<`");
    }

    #[test]
    fn empty_array_has_specific_error() {
        let l = Lexer::new(b"Array<>");
        let mut p = Parser::new(l).unwrap();
        let err = p.parse_type().unwrap_err();
        assert_eq!(err.msg, "Expected a type after `Array<`");
    }

    macro_rules! table {
        ($group:ident,$(($name:ident,$input:literal,$expected:expr))*) => {
            mod $group {
                use super::*;

                $(
                    #[test]
                    fn $name() {
                        let l = Lexer::new($input.as_bytes());
                        let p = Parser::new(l).unwrap();
                        let tt = p.parse().unwrap();
                        assert_eq!(tt, $expected);
                    }
                )*
            }
        };
    }

    table! {
        happy_path,
        (
            binding,
            "let variable_name = 5",
            vec![Node::Let {
                id: 0,
                name: mk_tok!(Type::Ident("variable_name")),
                rhs: Box::new(Node::Atom {
                    id: 1,
                    raw: mk_tok!(Type::I("5")),
                })
            }]
        )
        (
            function_with_explicit_void,
            "fn zero_args() {}",
            vec![Node::Fn {
                name: mk_tok!(Type::Ident("zero_args")),
                args: vec![],
                return_type: TypeExpr::Atom(mk_tok!(Type::Void)),
                body: vec![],
            }]
        )
        (
            function,
            "fn implicit_void() {}",
            vec![Node::Fn {
                name: mk_tok!(Type::Ident("implicit_void")),
                args: vec![],
                return_type: TypeExpr::Atom(mk_tok!(Type::Void)),
                body: vec![],
            }]
        )
        (
            foreign_function,
            "fn new(value: Foreign<Counter>) Foreign<Counter> {}",
            vec![Node::Fn {
                name: mk_tok!(Type::Ident("new")),
                args: vec![(
                    mk_tok!(Type::Ident("value")),
                    TypeExpr::Foreign(mk_tok!(Type::Ident("Counter"))),
                )],
                return_type: TypeExpr::Foreign(mk_tok!(Type::Ident("Counter"))),
                body: vec![],
            }]
        )
        (
            expression,
            "3+0.1415*5/27",
            vec![
                Node::Bin {
                    id: 6,
                    op: mk_tok!(Type::Plus),
                    lhs: Box::new(Node::Atom {
                        id: 0,
                        raw: mk_tok!(Type::I("3")),
                    }),
                    rhs: Box::new(Node::Bin {
                        id: 5,
                        op: mk_tok!(Type::Slash),
                        lhs: Box::new(Node::Bin {
                            id: 3,
                            op: mk_tok!(Type::Asteriks),
                            lhs: Box::new(Node::Atom {
                                id: 1,
                                raw: mk_tok!(Type::D("0.1415")),
                            }),
                            rhs: Box::new(Node::Atom {
                                id: 2,
                                raw: mk_tok!(Type::I("5")),
                            }),
                        }),
                        rhs: Box::new(Node::Atom {
                            id: 4,
                            raw: mk_tok!(Type::I("27")),
                        }),
                    }),
                }
            ]
        )
    }

    #[test]
    fn adjacent_parenthesized_arg_after_atom_is_not_postfix_call() {
        let l = Lexer::new(b"f(0.0 (1.0 + 2.0))");
        let p = Parser::new(l).unwrap();
        let tt = p.parse().unwrap();

        assert_eq!(
            tt,
            vec![Node::Call {
                id: 5,
                target: Box::new(Node::Ident {
                    id: 0,
                    name: mk_tok!(Type::Ident("f")),
                }),
                args: vec![
                    Node::Atom {
                        id: 1,
                        raw: mk_tok!(Type::D("0.0")),
                    },
                    Node::Bin {
                        id: 4,
                        op: mk_tok!(Type::Plus),
                        lhs: Box::new(Node::Atom {
                            id: 2,
                            raw: mk_tok!(Type::D("1.0")),
                        }),
                        rhs: Box::new(Node::Atom {
                            id: 3,
                            raw: mk_tok!(Type::D("2.0")),
                        }),
                    },
                ],
            }]
        );
    }

    #[test]
    fn equal_in_expression_terminates() {
        let l = Lexer::new(b"(5 = 6)");
        let p = Parser::new(l).unwrap();
        let result = p.parse();
        assert!(result.is_err(), "expected parse error, got: {result:?}");
    }
}
