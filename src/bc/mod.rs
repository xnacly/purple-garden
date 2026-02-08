use std::num;

mod ctx;
mod dis;
mod reg;

use crate::{
    Args,
    bc::{ctx::Context, reg::RegisterAllocator},
    err::PgError,
    ir::{Const, Func},
    vm::{CallFrame, Value, Vm, op::Op},
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

    fn emit(&mut self, op: Op) {
        self.buf.push(op)
    }

    fn load_const(&mut self, c: Const<'cc>) -> u8 {
        let dst = self.register.alloc();
        let idx = self.ctx.intern(c);
        self.emit(Op::LoadG { dst, idx });
        dst
    }

    pub fn compile(&mut self, ir: &'cc [Func<'cc>]) -> Result<(), PgError> {
        for func in ir {
            let _ = self.cc(func)?;
        }
        Ok(())
    }

    fn cc(&mut self, ir_node: &Func<'cc>) -> Result<Option<Reg>, PgError> {
        let f = ctx::Func {
            pc: self.buf.len(),
            name: ir_node.name,
        };
        self.ctx.functions.insert(ir_node.id, f);
        todo!()
    }

    pub fn finalize(self, config: &'cc Args) -> Vm<'cc> {
        let mut v = Vm::new(config);
        v.bytecode = self.buf;
        v.globals = self.ctx.globals_vec.into_iter().map(Value::from).collect();
        v.frames.push(CallFrame { return_to: 0 });
        v
    }
}
