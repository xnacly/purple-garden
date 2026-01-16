use crate::{
    ast::{InnerNode, Node},
    err::PgError,
    lex::{Lexer, Token, Type},
};

pub struct Parser<'p> {
    lex: Lexer<'p>,
}

impl<'p> Parser<'p> {
    pub fn new(lex: Lexer<'p>) -> Self {
        Self { lex }
    }

    pub fn parse(self) -> Result<Vec<Node<'p>>, PgError> {
        macro_rules! node {
            ($token:expr, $inner:expr) => {
                Node {
                    token: $token,
                    inner: $inner,
                }
            };
        }

        macro_rules! token {
            ($expr:expr) => {
                Token {
                    line: 0,
                    col: 0,
                    t: $expr,
                }
            };
        }
        let ast = [
            node! {
                token!(Type::Ident("sum_three")),
                InnerNode::Fn {
                    args: vec![
                        node!(token!(Type::Ident("a")), InnerNode::Ident),
                        node!(token!(Type::Ident("b")), InnerNode::Ident),
                        node!(token!(Type::Ident("c")), InnerNode::Ident),
                    ],
                    body: vec![
                        node!(
                            token!(Type::Plus),
                            InnerNode::Bin {
                                lhs: Box::new(node!(token!(Type::Ident("a")), InnerNode::Ident)),
                                rhs: Box::new(node!(
                                    token!(Type::Plus),
                                    InnerNode::Bin {
                                        lhs: Box::new(node!(token!(Type::Ident("b")), InnerNode::Ident)),
                                        rhs: Box::new(node!(token!(Type::Ident("c")), InnerNode::Ident)),
                                    }
                                )),
                            }
                        )
                    ],
                }
            },
            // Call the function with constants: sum_three(1, 2, 3)
            node! {
                token!(Type::Ident("sum_three")),
                InnerNode::Call {
                    args: vec![
                        node!(token!(Type::Integer("1")), InnerNode::Atom),
                        node!(token!(Type::Integer("2")), InnerNode::Atom),
                        node!(token!(Type::Integer("3")), InnerNode::Atom),
                    ],
                }
            },
        ];

        Ok(ast.to_vec())
    }
}
