use std::num;

mod ctx;
mod dis;
mod reg;

use crate::{
    Args,
    bc::{ctx::Context, reg::RegisterAllocator},
    err::PgError,
    ir::{self, Const, Func, TypeId},
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

    fn emit(&mut self, op: Op) -> usize {
        let pc = self.buf.len();
        self.buf.push(op);
        pc
    }

    fn replace(&mut self, idx: usize, op: Op) {
        self.buf[idx] = op
    }

    fn load_const(&mut self, c: Const<'cc>) -> u8 {
        let dst = self.register.alloc();
        let idx = self.ctx.intern(c);
        self.emit(Op::LoadG { dst, idx });
        dst
    }

    /// Compile a list of ir functions to bytecode instructions
    pub fn compile(&mut self, ir: &'cc [Func<'cc>]) -> Result<(), PgError> {
        for func in ir {
            let _ = self.cc(func)?;
        }
        Ok(())
    }

    fn cc(&mut self, fun: &Func<'cc>) -> Result<Option<Reg>, PgError> {
        // since we have a ssa based ir, we use our register allocator in a function local way and
        // spill any register usage >= 64 on the vm stack, this should be very fast for the general
        // usage and extensible enough for extreme niche usecases requiring more than 64 alive
        // values at the same time

        let pc = self.buf.len();
        let f = ctx::Func { pc, name: fun.name };
        // binding the id of a function to its context
        self.ctx.functions.insert(fun.id, f);

        // TODO: deal with registers still alive after a block transition, how? IDK :0
        for block in &fun.blocks {
            for instruction in &block.instructions {
                self.from_ir_instruction(&instruction);
            }

            // we dont want a termination for the entry point
            if let Some(term) = &block.term {
                match term {
                    ir::Terminator::Return(id) => {
                        // TODO: deal with return value, MUST be in r0
                        self.emit(Op::Ret);
                    }
                    _ => todo!("{:?}", &block.term),
                }
            }
        }

        crate::trace!(
            "[bc] compiled `{}` (size={})",
            fun.name,
            self.buf.len() - pc
        );

        Ok(None)
    }

    fn from_ir_instruction(&mut self, i: &ir::Instr<'cc>) {
        match i {
            ir::Instr::LoadConst { dst, value } => {
                let r_dst = self.register.alloc();
                if let Const::Int(i) = value
                    && *i < i32::MAX as i64
                {
                    self.emit(Op::LoadI {
                        dst: r_dst,
                        value: *i as i32,
                    });
                } else {
                    let idx = self.ctx.intern(value.clone());
                    self.emit(Op::LoadG { dst: r_dst, idx });
                }
            }
            ir::Instr::Call { dst, func, args } => {
                let Some(def_size_pc) = self.ctx.functions.get(func) else {
                    unreachable!();
                };

                // TODO: do some kind of ssa to register mapping so the function call has the
                // registers in r0..rN, also emit Op::Push{src:u8} for each alive register

                self.emit(Op::Call {
                    func: def_size_pc.pc as u32,
                });
            }
            ir::Instr::Add { dst, rhs, lhs }
            | ir::Instr::Sub { dst, rhs, lhs }
            | ir::Instr::Mul { dst, rhs, lhs }
            | ir::Instr::Div { dst, rhs, lhs }
            | ir::Instr::Eq { dst, rhs, lhs } => {
                let (
                    TypeId {
                        id: ir::Id(dst), ..
                    },
                    ir::Id(lhs),
                    ir::Id(rhs),
                ) = (dst, lhs, rhs);

                // rust really is bad at converting enums with shared payloads to other enums, what
                // even is this cluster fuck? I prob couldve done this better but i cant think of a
                // way :/
                macro_rules! emit_bin {
                    ($name:ident, $dst:expr, $lhs:expr, $rhs:expr) => {
                        Op::$name {
                            dst: (*$dst) as u8,
                            lhs: (*$lhs) as u8,
                            rhs: (*$rhs) as u8,
                        }
                    };
                }

                let op = match i {
                    ir::Instr::Add { .. } => emit_bin!(IAdd, dst, lhs, rhs),
                    ir::Instr::Sub { .. } => emit_bin!(ISub, dst, lhs, rhs),
                    ir::Instr::Mul { .. } => emit_bin!(IMul, dst, lhs, rhs),
                    ir::Instr::Div { .. } => emit_bin!(IDiv, dst, lhs, rhs),
                    ir::Instr::Eq { .. } => emit_bin!(Eq, dst, lhs, rhs),
                    _ => unreachable!(),
                };

                self.emit(op);
            }
            _ => todo!("{:?}", i),
        }
    }

    pub fn finalize(self, config: &'cc Args) -> Vm<'cc> {
        let mut v = Vm::new(config);
        v.pc = self
            .ctx
            .functions
            .get(&ir::Id(0))
            .map(|n| n.pc)
            .unwrap_or_default();
        v.bytecode = self.buf;
        v.globals = self.ctx.globals_vec.into_iter().map(Value::from).collect();
        v.frames.push(CallFrame { return_to: 0 });
        v
    }
}
