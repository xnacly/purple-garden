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
            token: Token {
                line: 0,
                col: 0,
                t: (Type::Asteriks),
            },
            inner: InnerNode::Bin {
                lhs: Box::new(Node {
                    token: (Token {
                        line: 0,
                        col: 0,
                        t: (Type::Plus),
                    }),
                    inner: (InnerNode::Bin {
                        lhs: Box::new(Node {
                            token: (Token {
                                line: 0,
                                col: 0,
                                t: (Type::Integer("2")),
                            }),
                            inner: (InnerNode::Atom),
                        }),
                        rhs: Box::new(Node {
                            token: (Token {
                                line: 0,
                                col: 0,
                                t: (Type::Integer("3")),
                            }),
                            inner: (InnerNode::Atom),
                        }),
                    }),
                }),
                rhs: Box::new(Node {
                    token: (Token {
                        line: 0,
                        col: 0,
                        t: (Type::Minus),
                    }),
                    inner: (InnerNode::Bin {
                        lhs: Box::new(Node {
                            token: (Token {
                                line: 0,
                                col: 0,
                                t: (Type::Integer("4")),
                            }),
                            inner: (InnerNode::Atom),
                        }),
                        rhs: Box::new(Node {
                            token: (Token {
                                line: 0,
                                col: 0,
                                t: (Type::Integer("1")),
                            }),
                            inner: (InnerNode::Atom),
                        }),
                    }),
                }),
            },
        };

        Ok(vec![ast])
    }
}
