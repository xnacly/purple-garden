use std::num;

mod ctx;
mod dis;
mod reg;

use crate::{
    ast::Node,
    cc::{
        ctx::{Context, Local},
        reg::RegisterAllocator,
    },
    err::PgError,
    ir::Const,
    lex::Type,
    op::Op,
    vm::{CallFrame, Value, Vm},
};

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
        Ok(match &ast {
            Node::Atom { raw } => {
                let constant = match &raw.t {
                    Type::I(s) => {
                        let value = s.parse().map_err(|e: num::ParseIntError| {
                            PgError::with_msg(e.to_string(), raw)
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
                    Type::D(s) => Const::Double(
                        s.parse::<f64>()
                            .map_err(|e: num::ParseFloatError| {
                                PgError::with_msg(e.to_string(), raw)
                            })?
                            .to_bits(),
                    ),
                    Type::S(s) => Const::Str(s),
                    Type::True => Const::True,
                    Type::False => Const::False,
                    _ => unreachable!(
                        "This is considered an impossible path, InnerNode::Atom can only have Type::{{Integer, Double, String, True, False}}"
                    ),
                };

                Some(self.load_const(constant).into())
            }
            Node::Ident { name } => {
                let Type::Ident(ident_name) = name.t else {
                    unreachable!("Node::Ident.name.t not Type::Ident, compiler bug");
                };

                Some(Reg {
                    id: self.ctx.locals.resolve(ident_name).ok_or_else(|| {
                        PgError::with_msg(format!("Undefined variable `{ident_name}`"), name)
                    })?,
                    perm: true,
                })
            }
            Node::Bin { op, lhs, rhs } => {
                let lhs_reg = self.cc(lhs.as_ref())?.unwrap();
                let rhs_reg = self.cc(rhs.as_ref())?.unwrap();

                let dst = self.register.alloc();
                let (lhs, rhs) = ((&lhs_reg).into(), (&rhs_reg).into());
                self.buf.push(match op.t {
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
            Node::Let { name, rhs } => {
                let src = self.cc(rhs)?;
                let Type::Ident(let_name) = name.t else {
                    unreachable!("Node::Let.name.t not Type::Ident, compiler bug");
                };

                self.ctx
                    .locals
                    .bind(let_name, src.unwrap().into())
                    .ok_or_else(|| {
                        PgError::with_msg(format!("binding `{let_name}` is already defined"), name)
                    })?;
                None
            }
            Node::Fn {
                name, args, body, ..
            } => {
                let prev_locals = std::mem::take(&mut self.ctx.locals);
                self.ctx.locals = Local::default();

                // we jump over function definitions, this is fast, works well for global scope and
                // keeps the compiler single pass

                let skip_jmp = self.buf.len();
                self.buf.push(Op::Jmp { target: 0 });

                let Type::Ident(fn_name) = name.t else {
                    unreachable!(
                        "This should be a Token::Ident, it was {:?}, this is a compiler bug",
                        name
                    );
                };

                let mut func = ctx::Func {
                    name: fn_name,
                    args: args.len() as u8,
                    size: 0,
                    pc: self.buf.len(),
                };

                self.register.mark();

                for (i, (arg, _)) in args.iter().enumerate() {
                    let Type::Ident(name) = arg.t else {
                        unreachable!("Function argument names must be identifiers, compiler bug");
                    };

                    // we need to reserver all registers from r0..rN, so the arguments arent
                    // clobbered by other operations
                    let _ = self.register.alloc();

                    self.ctx.locals.bind(name, i as u8).ok_or_else(|| {
                        PgError::with_msg(format!("binding `{name}` is already defined"), arg)
                    })?;
                }

                let mut result_register = None;
                for (i, field) in body.iter().enumerate() {
                    if let Some(r) = self.cc(field)?
                        && i == body.len() - 1
                    {
                        result_register = Some(r);
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

                self.ctx.functions.insert(fn_name, func);

                self.register.reset_to_last_mark();

                // restore outer locals:
                self.ctx.locals = prev_locals;
                None
            }
            Node::Call { name, args } => {
                let Type::Ident(call_name) = name.t else {
                    unreachable!("InnerNode::Call's token can only be an ident");
                };

                let resolved_func = self
                    .ctx
                    .functions
                    .get_mut(call_name)
                    .ok_or_else(|| {
                        PgError::with_msg(format!("function `{call_name}` is not defined"), name)
                    })?
                    .clone();

                if resolved_func.args != args.len() as u8 {
                    return Err(PgError::with_msg(
                        format!(
                            "function `{call_name}` requires {} arguments, got {}",
                            resolved_func.args,
                            args.len()
                        ),
                        name,
                    ));
                }

                self.register.mark();
                for (i, arg) in args.iter().enumerate() {
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
