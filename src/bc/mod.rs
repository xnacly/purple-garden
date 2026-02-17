use std::{collections::HashMap, num};

mod ctx;
pub mod dis;

use crate::{
    Args,
    bc::ctx::Context,
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
    pub ctx: Context<'cc>,
    /// binding a block id to its pc
    block_map: HashMap<ir::Id, u16>,
    /// prefilled block id to block
    blocks: HashMap<ir::Id, &'cc ir::Block<'cc>>,
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
            block_map: HashMap::new(),
            blocks: HashMap::new(),
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

    /// Compile a list of ir functions to bytecode instructions
    pub fn compile(&mut self, ir: &'cc [Func<'cc>]) -> Result<(), PgError> {
        for func in ir {
            let _ = self.cc(func)?;
        }
        Ok(())
    }

    fn cc(&mut self, fun: &'cc Func<'cc>) -> Result<Option<Reg>, PgError> {
        // since we have a ssa based ir, we use our register allocator in a function local way and
        // spill any register usage >= 64 on the vm stack, this should be very fast for the general
        // usage and extensible enough for extreme niche usecases requiring more than 64 alive
        // values at the same time

        let pc = self.buf.len();
        let f = ctx::Func { pc, name: fun.name };
        // binding the id of a function to its context
        self.ctx.functions.insert(fun.id, f);

        self.blocks = fun.blocks.iter().map(|b| (b.id, b)).collect();
        for block in &fun.blocks {
            self.block_map.insert(block.id, self.buf.len() as u16);

            for instruction in &block.instructions {
                self.from_instr(&instruction);
            }

            self.from_term(block.term.as_ref())
        }

        crate::trace!(
            "[bc] compiled `{}` (size={})",
            fun.name,
            self.buf.len() - pc
        );

        Ok(None)
    }

    fn from_term(&mut self, t: Option<&ir::Terminator>) {
        let Some(term) = t else {
            return;
        };

        match term {
            ir::Terminator::Return(id) => {
                // only insert a return value mov if the return value is not in r0
                if let Some(ir::Id(src)) = id
                    && src != &0
                {
                    self.emit(Op::Mov {
                        dst: 0,
                        src: *src as u8,
                    });
                }

                self.emit(Op::Ret);
            }
            ir::Terminator::Jump { id, params } => {
                let target = *self.blocks.get(id).unwrap();
                for (i, param) in params.iter().enumerate() {
                    let ir::Id(src) = param;
                    let ir::Id(dst) = target.params[i].id;

                    self.emit(Op::Mov {
                        dst: dst as u8,
                        src: *src as u8,
                    });
                }

                let ir::Id(id) = id;
                self.emit(Op::Jmp { target: *id as u16 });
            }
            ir::Terminator::Branch {
                cond,
                yes: ir::Id(yes),
                no: ir::Id(no),
            } => {
                let ir::Id(cond) = cond;
                self.emit(Op::JmpF {
                    cond: *cond as u8,
                    target: *yes as u16,
                });
                self.emit(Op::Jmp { target: *no as u16 });
            }
            _ => todo!("{:?}", &t),
        }
    }

    fn from_instr(&mut self, i: &ir::Instr<'cc>) {
        match i {
            ir::Instr::LoadConst { dst, value } => {
                let TypeId {
                    id: ir::Id(dst), ..
                } = dst;

                match value {
                    Const::Int(i) if *i < i32::MAX as i64 => {
                        self.emit(Op::LoadI {
                            dst: *dst as u8,
                            value: *i as i32,
                        });
                    }
                    _ => {
                        let idx = self.ctx.intern(value.clone());
                        self.emit(Op::LoadG {
                            dst: *dst as u8,
                            idx,
                        });
                    }
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

    pub fn finalize(mut self, config: &'cc Args) -> Vm<'cc> {
        let mut v = Vm::new(config);
        v.pc = self
            .ctx
            .functions
            .get(&ir::Id(0))
            .map(|n| n.pc)
            .unwrap_or_default();

        // second bytecode pass to resolve jumps from block Ids to bytecode positions
        for i in 0..self.buf.len() {
            let instr = self.buf[i];
            if let Some(new) = match instr {
                Op::JmpF { target, cond } => Some(Op::JmpF {
                    cond,
                    target: *self.block_map.get(&ir::Id(target as u32)).unwrap(),
                }),
                Op::Jmp { target } => Some(Op::Jmp {
                    target: *self.block_map.get(&ir::Id(target as u32)).unwrap(),
                }),
                _ => None,
            } {
                self.buf[i] = new
            }
        }

        v.bytecode = self.buf;
        v.globals = self.ctx.globals_vec.into_iter().map(Value::from).collect();
        v
    }

    pub fn function_table(&self) -> HashMap<usize, String> {
        self.ctx
            .functions
            .iter()
            .map(|(_, f)| (f.pc, f.name.to_string()))
            .collect()
    }
}
