use crate::{
    ast::{Node, TypeExpr},
    err::PgError,
    lex::{Lexer, Token, Type},
};

/// Parsing the token stream one token at a time into the abstract syntax tree, see
/// [ast.rs](./ast.rs) for documentation regarding each node and the way those should be parsed.
pub struct Parser<'p> {
    lex: &'p mut Lexer<'p>,
    id: usize,
    cur: Token<'p>,
}

impl<'p> Parser<'p> {
    pub fn new(lex: &'p mut Lexer<'p>) -> Result<Self, PgError> {
        let cur = lex.next()?;
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
            _ => self.parse_expr(0),
        }
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
        self.next()?;
        let name = self.expect_ident()?;

        self.expect(Type::BraceLeft)?;
        let mut args = vec![];
        while self.cur().t != Type::BraceRight {
            let arg_name = self.expect_ident()?;
            self.expect(Type::Colon)?;
            let arg_type = self.parse_type()?;
            args.push((arg_name, arg_type));
        }
        self.expect(Type::BraceRight)?;

        let return_type = self.parse_type()?;

        let mut body = vec![];
        self.expect(Type::CurlyLeft)?;
        while self.cur().t != Type::CurlyRight {
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
        todo!("Parser::parse_match");
    }

    fn parse_for(&mut self) -> Result<Node<'p>, PgError> {
        todo!("Parser::parse_for");
    }

    fn parse_expr(&mut self, min_bp: u8) -> Result<Node<'p>, PgError> {
        let mut lhs = match self.cur().t {
            Type::S(_) | Type::I(_) | Type::D(_) | Type::True | Type::False => {
                let raw = self.cur.clone();
                self.next()?;
                Node::Atom {
                    raw,
                    id: self.next_id(),
                }
            }
            Type::Ident(_) => {
                let name = self.cur.clone();
                self.next()?;
                // we are in a function call
                if self.cur().t == Type::BraceLeft {
                    self.next()?;
                    let mut args = vec![];
                    while self.cur().t != Type::BraceRight {
                        args.push(self.parse_prefix()?);
                    }
                    self.next()?;
                    Node::Call {
                        name,
                        args,
                        id: self.next_id(),
                    }
                } else {
                    Node::Ident {
                        name,
                        id: self.next_id(),
                    }
                }
            }
            Type::BraceLeft => {
                self.next()?;
                let e = self.parse_expr(0)?;
                self.expect(Type::BraceRight)?;
                e
            }
            Type::Plus | Type::Minus => {
                let rbp = Parser::prefix_binding_power(&self.cur().t);
                self.next()?;
                let _ = self.parse_expr(rbp)?;
                todo!("prefix operations")
            }
            _ => todo!("{:?}", self.cur().t),
        };

        while let Type::Plus
        | Type::Minus
        | Type::Asteriks
        | Type::Slash
        | Type::Equal
        | Type::DoubleEqual
        | Type::LessThan
        | Type::GreaterThan = self.cur().t
        {
            let op = self.cur().clone();

            if let Some((lbp, rbp)) = Parser::infix_binding_power(&op.t) {
                if lbp < min_bp {
                    break;
                }

                self.next()?;

                let rhs = self.parse_expr(rbp)?;
                lhs = Node::Bin {
                    id: self.next_id(),
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };

                continue;
            }
        }

        Ok(lhs)
    }

    fn prefix_binding_power(op: &Type) -> u8 {
        // add !<expr>,?<expr>, the latter being the try operator
        match op {
            Type::Plus | Type::Minus => 5,
            _ => unreachable!(),
        }
    }

    fn infix_binding_power(op: &Type) -> Option<(u8, u8)> {
        // TODO: add !=,==,?,>,<,<=,>=
        Some(match op {
            Type::Plus | Type::Minus => (1, 2),
            Type::Asteriks | Type::Slash => (3, 4),
            _ => return None,
        })
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
            Type::Str | Type::Int | Type::Bool | Type::Void | Type::Double => {
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

#[cfg(test)]
mod tests {
    use crate::{
        ast::{Node, TypeExpr},
        lex::{Lexer, Token, Type},
        parser::Parser,
    };

    macro_rules! mk_tok {
        ($type:expr) => {
            Token {
                line: 0,
                col: 0,
                t: $type,
            }
        };
    }

    macro_rules! table_parse_types {
        ($group:ident,$(($name:ident,$input:literal,$expected:expr))*) => {
            mod $group {
                use super::*;

                $(
                    #[test]
                    fn $name() {
                        let mut l = Lexer::new($input.as_bytes());
                        let mut p = Parser::new(&mut l).unwrap();
                        let tt = p.parse_type().unwrap();
                        assert_eq!(tt, $expected);
                    }
                )*
            }
        };
    }

    table_parse_types! {
        parse_types_atom,
        (int,"int",TypeExpr::Atom(mk_tok!(Type::Int)))
        (double,"double", TypeExpr::Atom(mk_tok!(Type::Double)))
        (str,"str", TypeExpr::Atom(mk_tok!(Type::Str)))
        (bool,"bool", TypeExpr::Atom(mk_tok!(Type::Bool)))
        (void,"void", TypeExpr::Atom(mk_tok!(Type::Void)))
    }

    table_parse_types! {
        parse_types_option,
        (int,"?int", TypeExpr::Option(Box::new(TypeExpr::Atom(mk_tok!(Type::Int)))))
        (double,"?double", TypeExpr::Option(Box::new(TypeExpr::Atom(mk_tok!(Type::Double)))))
        (str,"?str", TypeExpr::Option(Box::new(TypeExpr::Atom(mk_tok!(Type::Str)))))
        (bool,"?bool", TypeExpr::Option(Box::new(TypeExpr::Atom(mk_tok!(Type::Bool)))))
        (void,"?void", TypeExpr::Option(Box::new(TypeExpr::Atom(mk_tok!(Type::Void)))))
        (double_wrapped,"??void", TypeExpr::Option(Box::new(TypeExpr::Option(Box::new(TypeExpr::Atom(mk_tok!(Type::Void)))))))
    }

    table_parse_types! {
        parse_types_array,
        (int,"[int]", TypeExpr::Array(Box::new(TypeExpr::Atom(mk_tok!(Type::Int)))))
        (double,"[double]", TypeExpr::Array(Box::new(TypeExpr::Atom(mk_tok!(Type::Double)))))
        (str,"[str]", TypeExpr::Array(Box::new(TypeExpr::Atom(mk_tok!(Type::Str)))))
        (bool,"[bool]", TypeExpr::Array(Box::new(TypeExpr::Atom(mk_tok!(Type::Bool)))))
        (void,"[void]", TypeExpr::Array(Box::new(TypeExpr::Atom(mk_tok!(Type::Void)))))
        (double_wrapped,"[[void]]", TypeExpr::Array(Box::new(TypeExpr::Array(Box::new(TypeExpr::Atom(mk_tok!(Type::Void)))))))
    }

    table_parse_types! {
        parse_types_map,
        (str_to_int,"str[int]", TypeExpr::Map{
            key: Box::new(TypeExpr::Atom(mk_tok!(Type::Str))),
            value: Box::new(TypeExpr::Atom(mk_tok!(Type::Int))),
        })
        (int_to_str,"int[str]", TypeExpr::Map{
            key: Box::new(TypeExpr::Atom(mk_tok!(Type::Int))),
            value: Box::new(TypeExpr::Atom(mk_tok!(Type::Str))),
        })
        (int_to_optional_str,"int[?str]", TypeExpr::Map{
            key: Box::new(TypeExpr::Atom(mk_tok!(Type::Int))),
            value: Box::new(
                TypeExpr::Option(
                    Box::new(
                        TypeExpr::Atom(mk_tok!(Type::Str)),
                        )
                    )
                ),
        })
        (set_like_str_to_void,"str[void]", TypeExpr::Map{
            key: Box::new(TypeExpr::Atom(mk_tok!(Type::Str))),
            value: Box::new(TypeExpr::Atom(mk_tok!(Type::Void))),
        })
        (str_to_map_of_maps,"str[str[void]]", TypeExpr::Map{
            key: Box::new(TypeExpr::Atom(mk_tok!(Type::Str))),
            value: Box::new(TypeExpr::Map{
                key: Box::new(TypeExpr::Atom(mk_tok!(Type::Str))),
                value: Box::new(TypeExpr::Atom(mk_tok!(Type::Void)))
            }),
        })
    }

    macro_rules! table {
        ($group:ident,$(($name:ident,$input:literal,$expected:expr))*) => {
            mod $group {
                use super::*;

                $(
                    #[test]
                    fn $name() {
                        let mut l = Lexer::new($input.as_bytes());
                        let p = Parser::new(&mut l).unwrap();
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
                id: 1,
                name: mk_tok!(Type::Ident("variable_name")),
                rhs: Box::new(Node::Atom {
                    id: 0,
                    raw: mk_tok!(Type::I("5")),
                })
            }]
        )
        (
            function,
            "fn zero_args() void {}",
            vec![Node::Fn {
                name: mk_tok!(Type::Ident("zero_args")),
                args: vec![],
                return_type: TypeExpr::Atom(mk_tok!(Type::Void)),
                body: vec![],
            }]
        )
        (
            expression,
            "3+0.1415*5/27",
            vec![Node::Bin{
                id: 0,
                op: mk_tok!(Type::Plus),
                lhs: Box::new(Node::Atom { raw: mk_tok!(Type::I("3")),
                    id: 0,
                }),
                rhs: Box::new(Node::Bin{
                    id: 0,
                    op: mk_tok!(Type::Slash),
                    lhs: Box::new(Node::Bin{
                        id: 0,
                        op: mk_tok!(Type::Asteriks),
                        lhs: Box::new(Node::Atom { raw: mk_tok!(Type::D("0.1415")), id: 0,}),
                        rhs: Box::new(Node::Atom { raw: mk_tok!(Type::I("5")), id: 0 }),
                    }),
                    rhs: Box::new(Node::Atom { raw: mk_tok!(Type::I("27")), id: 0 }),
                })
            }]
        )
    }
}
