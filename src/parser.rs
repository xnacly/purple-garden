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
        let ast = Node {
            token: (Token {
                line: 0,
                col: 0,
                t: (Type::Ident("square")),
            }),
            inner: (InnerNode::Fn {
                args: vec![Node {
                    token: (Token {
                        line: 0,
                        col: 0,
                        t: (Type::Ident("n")),
                    }),
                    inner: (InnerNode::Ident),
                }],
                body: vec![Node {
                    token: (Token {
                        line: 0,
                        col: 0,
                        t: (Type::Asteriks),
                    }),
                    inner: (InnerNode::Bin {
                        lhs: Box::new(Node {
                            token: (Token {
                                line: 0,
                                col: 0,
                                t: (Type::Ident("n")),
                            }),
                            inner: (InnerNode::Ident),
                        }),
                        rhs: Box::new(Node {
                            token: (Token {
                                line: 0,
                                col: 0,
                                t: (Type::Ident("n")),
                            }),
                            inner: (InnerNode::Ident),
                        }),
                    }),
                }],
            }),
        };

        Ok(vec![ast])
    }
}
