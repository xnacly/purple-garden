#![allow(dead_code, unused_variables)]
#![cfg_attr(feature = "nightly", feature(likely_unlikely))]

use crate::{
    ast::{InnerNode, Node},
    err::PgError,
    lex::{Token, Type},
};

mod ast;
mod cc;
/// pretty print errors
mod err;
/// simple mark and sweep garbage collector, will be replaced by a manchester garbage collector in
/// the future
mod gc;
mod lex;
/// purple garden bytecode virtual machine operations
mod op;
mod parser;
/// register based virtual machine
mod vm;

// TODO:
// - port pg cli to serde
// - port frontend (lexer, parser)
//      - port tokens
//      - port ast
// - port cc
// - port vm fully
// - port gc
// - implement very good errors
// - build peephole optimisation (constant folding, load elim) and x86 jit, can be enabled via -O1
// - build optimising x86 jit with ssa via -O2
// - allow for writing bytecode to disk
fn main() {
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

    let mut cc = cc::Cc::new();
    if let Err(e) = cc.compile(ast) {
        e.render();
        std::process::exit(1);
    }

    let mut vm = cc.finalize();
    if let Err(e) = vm.run() {
        Into::<PgError>::into(e).render();
    }

    dbg!(&vm.registers[0]);
}
