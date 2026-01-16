use std::{hash::Hash, num};

mod ctx;
mod dis;
mod reg;

use crate::{
    ast::{InnerNode, Node},
    cc::{
        ctx::{Context, Local},
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

#[derive(Debug, PartialEq, Eq)]
struct Reg {
    id: u8,
    perm: bool,
}

impl From<u8> for Reg {
    fn from(value: u8) -> Self {
        Reg {
            id: value,
            perm: false,
        }
    }
}

impl From<Reg> for u8 {
    fn from(value: Reg) -> Self {
        value.id
    }
}

impl From<&Reg> for u8 {
    fn from(value: &Reg) -> Self {
        value.id
    }
}

#[derive(Debug)]
pub struct Cc<'cc> {
    pub buf: Vec<Op>,
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

    pub fn compile(&mut self, ast: &'cc [Node<'cc>]) -> Result<(), PgError> {
        for n in ast {
            let _ = self.cc(n)?;
        }
        Ok(())
    }

    fn cc(&mut self, ast: &Node<'cc>) -> Result<Option<Reg>, PgError> {
        #[cfg(feature = "trace")]
        println!("Cc::cc({:?})", &ast.token.t);

        Ok(match &ast.inner {
            InnerNode::Atom => {
                let constant = match &ast.token.t {
                    Type::Integer(s) => {
                        let value = s.parse().map_err(|e: num::ParseIntError| {
                            PgError::with_msg(e.to_string(), &ast.token)
                        })?;

                        if value > i32::MAX as i64 {
                            Const::Int(value)
                        } else {
                            let r = self.register.alloc();
                            self.buf.push(Op::LoadImm {
                                dst: r,
                                value: value as i32,
                            });

                            // early bail, since we do LoadG for the other values
                            return Ok(Some(r.into()));
                        }
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

                Some(self.load_const(constant).into())
            }
            InnerNode::Ident => {
                let Type::Ident(name) = ast.token.t else {
                    unreachable!("InnerNode::Ident");
                };

                Some(Reg {
                    id: self.ctx.locals.resolve(name).ok_or_else(|| {
                        PgError::with_msg(format!("Undefined variable `{name}`"), &ast.token)
                    })?,
                    perm: true,
                })
            }
            InnerNode::Bin { lhs, rhs } => {
                let lhs_reg = self.cc(lhs.as_ref())?.unwrap();
                let rhs_reg = self.cc(rhs.as_ref())?.unwrap();

                let dst = self.register.alloc();
                let (lhs, rhs) = ((&lhs_reg).into(), (&rhs_reg).into());
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

                if let Reg { id, perm: false } = lhs_reg {
                    self.register.free(id);
                };
                if let Reg { id, perm: false } = rhs_reg {
                    self.register.free(id);
                };

                Some(dst.into())
            }
            InnerNode::Let { rhs } => {
                let src = self.cc(&rhs)?;
                let Type::Ident(name) = ast.token.t else {
                    unreachable!("InnerNode::Let");
                };

                self.ctx
                    .locals
                    .bind(name, src.unwrap().into())
                    .ok_or_else(|| {
                        PgError::with_msg(
                            format!("binding `{name}` is already defined"),
                            &ast.token,
                        )
                    })?;
                None
            }
            InnerNode::Fn { args, body } => {
                let prev_locals = std::mem::take(&mut self.ctx.locals);
                self.ctx.locals = Local::default();

                // we jump over function definitions, this is fast, works well for global scope and
                // keeps the compiler single pass

                let skip_jmp = self.buf.len();
                self.buf.push(Op::Jmp { target: 0 });

                let Type::Ident(name) = ast.token.t else {
                    unreachable!(
                        "This should be a Token::Ident, it was {:?}, this is a compiler bug",
                        ast.token.t
                    );
                };

                let mut func = ctx::Func {
                    name,
                    args: args.len() as u8,
                    size: 0,
                    pc: self.buf.len(),
                };

                self.register.mark();

                for (i, arg) in args.into_iter().enumerate() {
                    let Type::Ident(name) = arg.token.t else {
                        unreachable!("Function argument names must be identifiers");
                    };

                    // we need to reserver all registers from r0..rN, so the arguments arent
                    // clobbered by other operations
                    let _ = self.register.alloc();

                    self.ctx.locals.bind(name, i as u8).ok_or_else(|| {
                        PgError::with_msg(
                            format!("binding `{name}` is already defined"),
                            &ast.token,
                        )
                    })?;
                }

                let mut result_register = None;
                for (i, field) in body.iter().enumerate() {
                    if let Some(r) = self.cc(field)? {
                        if i == body.len() - 1 {
                            result_register = Some(r);
                        }
                    }
                }

                // adhere to calling convention if we have a value to return, it needs to be in r0
                if let Some(src) = result_register {
                    self.buf.push(Op::Mov {
                        dst: 0,
                        src: src.into(),
                    });
                }
                self.buf.push(Op::Ret);

                func.size = self.buf.len() - func.pc;
                self.buf[skip_jmp] = Op::Jmp {
                    target: self.buf.len() as u16,
                };

                self.ctx.functions.insert(name, func);

                self.register.reset_to_last_mark();

                // restore outer locals:
                self.ctx.locals = prev_locals;
                None
            }
            InnerNode::Call { args } => {
                let Type::Ident(name) = ast.token.t else {
                    unreachable!("InnerNode::Call's token can only be an ident");
                };

                let resolved_func = self
                    .ctx
                    .functions
                    .get_mut(name)
                    .ok_or_else(|| {
                        PgError::with_msg(format!("function `{name}` is not defined"), &ast.token)
                    })?
                    .clone();

                if resolved_func.args != args.len() as u8 {
                    return Err(PgError::with_msg(
                        format!(
                            "function `{name}` requires {} arguments, got {}",
                            resolved_func.args,
                            args.len()
                        ),
                        &ast.token,
                    ));
                }

                self.register.mark();
                for (i, arg) in args.into_iter().enumerate() {
                    let r = self.cc(arg)?;
                    if let Some(r) = r {
                        self.buf.push(Op::Mov {
                            dst: i as u8,
                            src: r.into(),
                        });
                    }
                }
                self.buf.push(Op::Call {
                    func: resolved_func.pc as u32,
                });
                self.register.reset_to_last_mark();
                Some(0.into())
            }
            _ => todo!("{:?}", ast),
        })
    }

    pub fn finalize(self) -> Vm<'cc> {
        let mut v = Vm::new();
        v.bytecode = self.buf;
        v.globals = self.ctx.globals_vec.into_iter().map(Value::from).collect();
        v.frames.push(CallFrame { return_to: 0 });
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
    /// (fn sum_three (a b c) (+ a (+ b c)))
    fn three_arg_call() {
        let ast = vec![
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

        let mut cc = Cc::new();
        let _ = cc.compile(&ast).unwrap();

        assert_eq!(
            cc.buf,
            vec![
                Op::Jmp { target: 5 },
                Op::Add {
                    dst: 0,
                    lhs: 1,
                    rhs: 2
                },
                Op::Add {
                    dst: 1,
                    lhs: 0,
                    rhs: 0
                },
                Op::Mov { dst: 0, src: 1 },
                Op::Ret,
                Op::LoadImm { dst: 0, value: 1 },
                Op::Mov { dst: 0, src: 0 },
                Op::LoadImm { dst: 1, value: 2 },
                Op::Mov { dst: 1, src: 1 },
                Op::LoadImm { dst: 2, value: 3 },
                Op::Mov { dst: 2, src: 2 },
                Op::Call { func: 1 }
            ]
        );
    }

    #[test]
    fn single_arg_call() {
        let ast = vec![
            node! {
                token!(Type::Ident("indirecton")),
                InnerNode::Fn {
                    args: vec![node!(token!(Type::Ident("x")), InnerNode::Ident)],
                    body: vec![node!(token!(Type::Ident("x")), InnerNode::Ident)],
                }
            },
            node! {
                token!(Type::Ident("indirecton")),
                InnerNode::Call {
                    args: vec![node!(token!(Type::Integer("161")), InnerNode::Atom)],
                }
            },
        ];

        let mut cc = Cc::new();
        let _ = cc.compile(&ast).unwrap();
        assert_eq!(
            cc.buf,
            vec![
                Op::Jmp { target: 3 },
                Op::Mov { dst: 0, src: 0 },
                Op::Ret,
                Op::LoadImm { dst: 0, value: 161 },
                Op::Mov { dst: 0, src: 0 },
                Op::Call { func: 1 }
            ]
        );
    }

    #[test]
    fn empty_call() {
        let ast = vec![
            node! {
                token!(Type::Ident("empty")),
                InnerNode::Fn {
                    args: vec![],
                    body: vec![],
                }
            },
            node! {
                token!(Type::Ident("empty")),
                InnerNode::Call {
                    args: vec![],
                }
            },
        ];

        let mut cc = Cc::new();
        let _ = cc.compile(&ast).unwrap();
        assert_eq!(
            cc.buf,
            vec![Op::Jmp { target: 2 }, Op::Ret, Op::Call { func: 1 }]
        );
    }

    #[test]
    fn undefined_call() {
        let ast = vec![node! {
            token!(Type::Ident("unkown")),
            InnerNode::Call {
                args: vec![],
            }
        }];

        let mut cc = Cc::new();
        assert!(dbg!(cc.compile(&ast)).is_err());
    }

    #[test]
    fn empty_function() {
        let ast = vec![node! {
            token!(Type::Ident("empty")),
            InnerNode::Fn {
                args: vec![],
                body: vec![],
            }
        }];

        let mut cc = Cc::new();
        let _ = cc.compile(&ast).unwrap();
        assert_eq!(cc.buf, vec![Op::Jmp { target: 2 }, Op::Ret]);
    }

    #[test]
    fn single_arg_function() {
        let ast = vec![node! {
            token!(Type::Ident("square")),
            InnerNode::Fn {
                args: vec![
                    node!{
                        token!(Type::Ident("n")),
                        InnerNode::Ident
                    },
                ],
                body: vec![
                    node!(
                        token!(Type::Asteriks),
                        InnerNode::Bin {
                            lhs: Box::new(node!(token!(Type::Ident("n")), InnerNode::Ident)),
                            rhs: Box::new(node!(token!(Type::Ident("n")), InnerNode::Ident)),
                        }
                    ),
                ],
            }
        }];

        let mut cc = Cc::new();
        let _ = cc.compile(&ast).unwrap();
        assert_eq!(
            cc.buf,
            vec![
                Op::Jmp { target: 4 },
                Op::Mul {
                    dst: 0,
                    lhs: 0,
                    rhs: 0
                },
                Op::Mov { dst: 0, src: 0 },
                Op::Ret
            ]
        );
    }

    #[test]
    fn atom_false() {
        let mut cc = Cc::new();
        let ast = vec![Node {
            token: token!(Type::False),
            inner: InnerNode::Atom,
        }];

        let _ = cc.compile(&ast).expect("Failed to compile node");
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
        let ast = vec![Node {
            token: token!(Type::True),
            inner: InnerNode::Atom,
        }];

        let _ = cc.compile(&ast).expect("Failed to compile node");
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
        let ast = vec![Node {
            token: token!(Type::String("hola")),
            inner: InnerNode::Atom,
        }];

        let _ = cc.compile(&ast).expect("Failed to compile node");
        assert_eq!(
            cc.buf,
            vec![Op::LoadGlobal {
                dst: 0,
                idx: cc.ctx.globals_vec.len() as u32 - 1
            }],
        );
        assert_eq!(cc.ctx.globals_vec.last(), Some(&Const::Str("hola")));
    }

    #[test]
    fn atom_int() {
        let mut cc = Cc::new();
        let ast = vec![Node {
            token: token!(Type::Integer("25")),
            inner: InnerNode::Atom,
        }];
        let _ = cc.compile(&ast).expect("Failed to compile node");
        assert_eq!(cc.buf, vec![Op::LoadImm { dst: 0, value: 25 }],);
    }

    #[test]
    fn atom_too_large_for_i32() {
        let mut cc = Cc::new();
        let inner = i64::from(i32::MAX) + 1;
        let inner_as_str = inner.to_string();
        let ast = vec![Node {
            token: token!(Type::Integer(&inner_as_str)),
            inner: InnerNode::Atom,
        }];
        let _ = cc.compile(&ast).expect("Failed to compile node");
        assert_eq!(
            cc.buf,
            vec![Op::LoadGlobal {
                dst: 0,
                idx: cc.ctx.globals_vec.len() as u32 - 1
            }],
        );
        assert_eq!(cc.ctx.globals_vec.last(), Some(&Const::Int(inner)));
    }

    #[test]
    fn atom_double() {
        let mut cc = Cc::new();
        let ast = vec![Node {
            token: token!(Type::Double("3.1415")),
            inner: InnerNode::Atom,
        }];
        let _ = cc.compile(&ast).expect("Failed to compile node");
        assert_eq!(
            cc.buf,
            vec![Op::LoadGlobal {
                dst: 0,
                idx: cc.ctx.globals_vec.len() as u32 - 1
            }],
        );
        assert_eq!(
            cc.ctx.globals_vec.last(),
            Some(&Const::Double((3.1415_f64).to_bits()))
        );
    }

    #[test]
    fn atom_ident_undefined_is_error() {
        let mut cc = Cc::new();

        let ast = vec![node! {
            token!(Type::Ident("x")),
            InnerNode::Ident
        }];

        assert!(cc.compile(&ast).is_err());
    }

    #[test]
    fn r#let() {
        let mut cc = Cc::new();

        let ast = vec![node! {
            token!(Type::Ident("x")),
            InnerNode::Let {
                rhs: Box::new(node!{
                    token!(Type::Integer("25")),
                    InnerNode::Atom
                }),
            }
        }];

        let _ = cc.compile(&ast).expect("Failed to compile node");
        let src: u8 = 0;
        assert_eq!(cc.buf, vec![Op::LoadImm { dst: 0, value: 25 },],);
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

        let _ = cc.compile(&ast).expect("Failed to compile node");

        assert_eq!(
            cc.buf,
            vec![Op::LoadImm {
                dst: 0,
                value: -5000
            }],
        );
    }

    #[test]
    fn let_redefinition_is_error() {
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
                InnerNode::Let {
                    rhs: Box::new(node!{
                        token!(Type::Integer("-5000")),
                        InnerNode::Atom
                    }),
                }
            },
        ];

        assert!(cc.compile(&ast).is_err());
    }

    #[test]
    fn multiple_lets_distinct_registers() {
        let mut cc = Cc::new();

        let ast = vec![
            node! {
                token!(Type::Ident("x")),
                InnerNode::Let {
                    rhs: Box::new(node!{
                        token!(Type::Integer("161")),
                        InnerNode::Atom
                    }),
                }
            },
            node! {
                token!(Type::Ident("y")),
                InnerNode::Let {
                    rhs: Box::new(node!{
                        token!(Type::Integer("187")),
                        InnerNode::Atom
                    }),
                }
            },
        ];

        let _ = cc.compile(&ast).unwrap();

        assert!(matches!(cc.buf[0], Op::LoadImm { dst: 0, value: 161 }));
        assert!(matches!(cc.buf[1], Op::LoadImm { dst: 1, value: 187 }));
    }

    #[test]
    fn ident_emits_no_ops() {
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

        cc.compile(&ast).unwrap();

        assert_eq!(cc.buf.len(), 1);
    }

    #[test]
    fn bin() {
        use crate::lex::Type::*;
        use crate::op::Op::*;

        let tests: Vec<(Type, fn(u8, u8, u8) -> Op)> = vec![
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
            let _ = cc.cc(&ast).expect("Failed to compile node");
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
        let ast = vec![Node {
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
        }];
        let mut cc = Cc::new();
        let _ = cc.compile(&ast).expect("Failed to compile node");
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
