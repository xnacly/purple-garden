use std::{hash::Hash, num};

mod ctx;
mod dis;
mod reg;

use crate::{
    ast::{InnerNode, Node},
    cc::{
        ctx::{Context, Locals},
        reg::RegisterAllocator,
    },
    err::PgError,
    lex::Type,
    op::Op,
    vm::{CallFrame, Value, Vm},
};

/// Compile time Value representation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub enum Const<'c> {
    False,
    True,
    Int(i64),
    Double(u64),
    Str(&'c str),
}

#[derive(Debug)]
pub struct Cc<'cc> {
    buf: Vec<Op<'cc>>,
    ctx: Context<'cc>,
    register: RegisterAllocator,
}

impl<'cc> Cc<'cc> {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(256),
            ctx: {
                let mut ctx = Context::default();
                ctx.intern(Const::False);
                ctx.intern(Const::True);
                ctx
            },
            register: RegisterAllocator::new(),
        }
    }

    pub const GLOBAL_FALSE: u32 = 0;
    pub const GLOBAL_TRUE: u32 = 1;

    fn load_const(&mut self, c: Const<'cc>) -> u8 {
        let r = self.register.alloc();
        self.buf.push(Op::LoadGlobal {
            dst: r,
            idx: self.ctx.intern(c),
        });
        r
    }

    /// compile is a simple wrapper around self.cc to make sure all registers are deallocated after their lifetime ends
    pub fn compile(&mut self, ast: Node<'cc>) -> Result<(), PgError> {
        let register = self.cc(ast)?;
        self.register.free(register);
        Ok(())
    }

    fn cc(&mut self, ast: Node<'cc>) -> Result<u8, PgError> {
        #[cfg(feature = "trace")]
        println!("Cc::cc({:?})", &ast.token.t);

        Ok(match ast.inner {
            InnerNode::Atom => {
                let constant = match &ast.token.t {
                    Type::Integer(s) => {
                        let value = s.parse().map_err(|e: num::ParseIntError| {
                            PgError::with_msg(e.to_string(), &ast.token)
                        })?;

                        let r = self.register.alloc();
                        self.buf.push(Op::LoadImm { dst: r, value });

                        // early bail, since we do LoadG for the other values
                        return Ok(r);
                    }
                    Type::Double(s) => Const::Double(
                        s.parse::<f64>()
                            .map_err(|e: num::ParseFloatError| {
                                PgError::with_msg(e.to_string(), &ast.token)
                            })?
                            .to_bits(),
                    ),
                    Type::String(s) => Const::Str(s),
                    Type::True => Const::True,
                    Type::False => Const::False,
                    _ => unreachable!(
                        "This is considered an impossible path, InnerNode::Atom can only have Type::{{Integer, Double, String, True, False}}"
                    ),
                };

                self.load_const(constant)
            }
            InnerNode::Ident => {
                let Type::Ident(name) = ast.token.t else {
                    unreachable!("InnerNode::Ident");
                };

                let slot = self.ctx.locals.resolve(name).ok_or_else(|| {
                    PgError::with_msg(format!("Undefined variable `{name}`"), &ast.token)
                })?;
                let dst = self.register.alloc();
                self.buf.push(Op::LoadLocal { dst, slot });
                dst
            }
            InnerNode::Bin { lhs, rhs } => {
                let lhs = self.cc(*lhs)?;
                let rhs = self.cc(*rhs)?;

                let dst = self.register.alloc();
                self.buf.push(match ast.token.t {
                    Type::Plus => Op::Add { dst, lhs, rhs },
                    Type::Minus => Op::Sub { dst, lhs, rhs },
                    Type::Asteriks => Op::Mul { dst, lhs, rhs },
                    Type::Slash => Op::Div { dst, lhs, rhs },
                    Type::LessThan => Op::Lt { dst, lhs, rhs },
                    Type::GreaterThan => Op::Gt { dst, lhs, rhs },
                    Type::Equal => Op::Eq { dst, lhs, rhs },
                    _ => unreachable!(),
                });

                self.register.free(lhs);
                self.register.free(rhs);
                dst
            }
            InnerNode::Let { rhs } => {
                let src = self.cc(*rhs)?;
                let Type::Ident(name) = ast.token.t else {
                    unreachable!("InnerNode::Let");
                };
                let slot = self.ctx.locals.define(name).ok_or_else(|| {
                    PgError::with_msg(format!("`{name}` is already defined"), &ast.token)
                })?;

                self.buf.push(Op::StoreLocal { slot, src });
                src
            }
            InnerNode::Fn { args, body } => {
                self.ctx.locals = Locals::default();
                todo!("Cc::InnerNode::Fn");
            }
            _ => todo!("{:?}", ast),
        })
    }

    pub fn finalize(self) -> Vm<'cc> {
        let mut v = Vm::new();
        v.bytecode = self.buf;
        v.globals = self.ctx.globals_vec.into_iter().map(Value::from).collect();
        v.frames.push(CallFrame {
            return_to: 0,
            locals_base: 0,
        });
        v
    }
}

#[cfg(test)]
mod cc {
    use crate::{
        ast::{InnerNode, Node},
        cc::{Cc, Const},
        lex::{Token, Type},
        op::Op,
    };

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

    #[test]
    fn atom_false() {
        let mut cc = Cc::new();
        let ast = Node {
            token: token!(Type::False),
            inner: InnerNode::Atom,
        };

        let _ = cc.compile(ast).expect("Failed to compile node");
        let expected_idx: usize = 0;
        assert_eq!(
            cc.buf,
            vec![Op::LoadGlobal {
                dst: 0,
                idx: expected_idx as u32
            }],
        );
        assert_eq!(cc.ctx.globals_vec[expected_idx], Const::False);
    }

    #[test]
    fn atom_true() {
        let mut cc = Cc::new();
        let ast = Node {
            token: token!(Type::True),
            inner: InnerNode::Atom,
        };

        let _ = cc.compile(ast).expect("Failed to compile node");
        let expected_idx: usize = 1;
        assert_eq!(
            cc.buf,
            vec![Op::LoadGlobal {
                dst: 0,
                idx: expected_idx as u32
            }],
        );
        assert_eq!(cc.ctx.globals_vec[expected_idx], Const::True);
    }

    #[test]
    fn atom_string() {
        let mut cc = Cc::new();
        let ast = Node {
            token: token!(Type::String("hola")),
            inner: InnerNode::Atom,
        };

        let _ = cc.compile(ast).expect("Failed to compile node");
        let expected_idx: usize = 2;
        assert_eq!(
            cc.buf,
            vec![Op::LoadGlobal {
                dst: 0,
                idx: expected_idx as u32
            }],
        );
        assert_eq!(cc.ctx.globals_vec[expected_idx], Const::Str("hola"));
    }

    #[test]
    fn atom_int() {
        let mut cc = Cc::new();
        let ast = Node {
            token: token!(Type::Integer("25")),
            inner: InnerNode::Atom,
        };
        let _ = cc.compile(ast).expect("Failed to compile node");
        assert_eq!(cc.buf, vec![Op::LoadImm { dst: 0, value: 25 }],);
    }

    #[test]
    fn atom_double() {
        let mut cc = Cc::new();
        let ast = Node {
            token: token!(Type::Double("3.1415")),
            inner: InnerNode::Atom,
        };
        let _ = cc.compile(ast).expect("Failed to compile node");
        let expected_idx: usize = 2;
        assert_eq!(
            cc.buf,
            vec![Op::LoadGlobal {
                dst: 0,
                idx: expected_idx as u32
            }],
        );
        assert_eq!(
            cc.ctx.globals_vec[expected_idx],
            Const::Double((3.1415_f64).to_bits())
        );
    }

    #[test]
    fn atom_ident() {
        let mut cc = Cc::new();
        cc.ctx.locals.define("thisisavariablename").unwrap();
        let name = "thisisavariablename";
        let ast = Node {
            token: token!(Type::Ident(name)),
            inner: InnerNode::Ident,
        };
        let _ = cc.compile(ast).expect("Failed to compile node");
        let expected_slot: u16 = 0;
        assert_eq!(
            cc.buf,
            vec![Op::LoadLocal {
                dst: 0,
                slot: expected_slot
            }],
        );
    }

    #[test]
    fn atom_ident_undefined_is_error() {
        let mut cc = Cc::new();

        let ast = node! {
            token!(Type::Ident("x")),
            InnerNode::Ident
        };

        assert!(cc.compile(ast).is_err());
    }

    #[test]
    fn r#let() {
        let mut cc = Cc::new();

        let ast = node! {
            token!(Type::Ident("x")),
            InnerNode::Let {
                rhs: Box::new(node!{
                    token!(Type::Integer("25")),
                    InnerNode::Atom
                }),
            }
        };

        let _ = cc.compile(ast).expect("Failed to compile node");
        let src: u8 = 0;

        assert_eq!(
            cc.buf,
            vec![
                Op::LoadImm { dst: 0, value: 25 },
                Op::StoreLocal { slot: 0, src }
            ],
        );
    }

    #[test]
    fn r#let_and_ident() {
        let mut cc = Cc::new();

        let ast = vec![
            node! {
                token!(Type::Ident("x")),
                InnerNode::Let {
                    rhs: Box::new(node!{
                        token!(Type::Integer("-5000")),
                        InnerNode::Atom
                    }),
                }
            },
            node! {
                token!(Type::Ident("x")),
                InnerNode::Ident
            },
        ];

        for node in ast {
            let _ = cc.compile(node).expect("Failed to compile node");
        }

        assert_eq!(
            cc.buf,
            vec![
                Op::LoadImm {
                    dst: 0,
                    value: -5000
                },
                Op::StoreLocal { slot: 0, src: 0 },
                Op::LoadLocal { slot: 0, dst: 0 },
            ],
        );
    }

    #[test]
    fn bin() {
        use crate::lex::Type::*;
        use crate::op::Op::*;

        let tests: Vec<(Type, fn(u8, u8, u8) -> Op<'static>)> = vec![
            (Plus, |dst, lhs, rhs| Add { dst, lhs, rhs }),
            (Minus, |dst, lhs, rhs| Sub { dst, lhs, rhs }),
            (Asteriks, |dst, lhs, rhs| Mul { dst, lhs, rhs }),
            (Slash, |dst, lhs, rhs| Div { dst, lhs, rhs }),
            (Equal, |dst, lhs, rhs| Eq { dst, lhs, rhs }),
            (LessThan, |dst, lhs, rhs| Lt { dst, lhs, rhs }),
            (GreaterThan, |dst, lhs, rhs| Gt { dst, lhs, rhs }),
        ];

        for (token_type, make_op) in tests {
            let mut cc = Cc::new();

            let ast = Node {
                token: token!(token_type.clone()),
                inner: InnerNode::Bin {
                    lhs: Box::new(node!(token!(Type::Integer("45")), InnerNode::Atom)),
                    rhs: Box::new(node!(token!(Type::Integer("45")), InnerNode::Atom)),
                },
            };
            let _ = cc.compile(ast).expect("Failed to compile node");
            let expected_op = make_op(2, 0, 1);
            assert_eq!(
                cc.buf,
                vec![
                    Op::LoadImm { dst: 0, value: 45 },
                    Op::LoadImm { dst: 1, value: 45 },
                    expected_op,
                ],
                "Failed for operator: {:?}",
                token_type
            );
        }
    }

    #[test]
    fn bin_nested() {
        let ast = Node {
            token: token!(Type::Asteriks),
            inner: InnerNode::Bin {
                lhs: Box::new(node!(
                    token!(Type::Plus),
                    InnerNode::Bin {
                        lhs: Box::new(node!(token!(Type::Integer("2")), InnerNode::Atom)),
                        rhs: Box::new(node!(token!(Type::Integer("3")), InnerNode::Atom)),
                    }
                )),
                rhs: Box::new(node!(
                    token!(Type::Minus),
                    InnerNode::Bin {
                        lhs: Box::new(node!(token!(Type::Integer("4")), InnerNode::Atom)),
                        rhs: Box::new(node!(token!(Type::Integer("1")), InnerNode::Atom)),
                    }
                )),
            },
        };
        let mut cc = Cc::new();
        let _ = cc.compile(ast).expect("Failed to compile node");
        assert_eq!(
            cc.buf,
            vec![
                Op::LoadImm { dst: 0, value: 2 },
                Op::LoadImm { dst: 1, value: 3 },
                Op::Add {
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                },
                Op::LoadImm { dst: 1, value: 4 },
                Op::LoadImm { dst: 0, value: 1 },
                Op::Sub {
                    dst: 3,
                    lhs: 1,
                    rhs: 0,
                },
                Op::Mul {
                    dst: 0,
                    lhs: 2,
                    rhs: 3,
                },
            ]
        )
    }
}
