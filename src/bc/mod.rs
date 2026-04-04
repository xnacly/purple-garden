use std::collections::HashMap;

pub mod dis;
mod intern;
mod regalloc;

use crate::{
    bc::{intern::Interner, regalloc::Ralloc},
    config::Config,
    err::PgError,
    ir::{self, Const, Func, Id, TypeId, ptype},
    std::{self as pstd, Fn, Pkg, STD},
    vm::{BuiltinFn, Value, Vm, op::Op},
};

#[derive(Debug, Clone)]
pub struct BcFunc<'fun> {
    pub name: &'fun str,
    pub pc: usize,
}

#[derive(Debug, Clone)]
pub struct Cc<'cc> {
    pub buf: Vec<Op>,
    pub globals: Interner<Const<'cc>>,
    pub strings: Interner<&'cc str>,
    pub std_fns: Interner<BuiltinFn>,
    pub functions: HashMap<Id, BcFunc<'cc>>,
    /// binding a block id to its pc
    block_map: HashMap<ir::Id, u16>,
    regalloc: Ralloc,
}

impl<'cc> Cc<'cc> {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(64),
            globals: Interner::new(),
            strings: Interner::new(),
            std_fns: Interner::new(),
            functions: HashMap::new(),
            block_map: HashMap::new(),
            regalloc: Ralloc::default(),
        }
    }

    fn intern(&mut self, constant: Const<'cc>) -> u32 {
        if let Const::Str(str) = constant {
            let str_pool_idx = self.strings.intern(str);
            self.globals.intern(Const::Int(str_pool_idx as i64))
        } else {
            self.globals.intern(constant)
        }
    }

    fn emit(&mut self, op: Op) -> usize {
        let pc = self.buf.len();
        self.buf.push(op);
        pc
    }

    fn ensure_register(&self, Id(ref id): Id) -> u8 {
        let Some(location) = self.regalloc.map.get(id) else {
            unreachable!(
                "Attempted a register alloc lookup for a not defined ssa virtual register %v{}",
                id
            );
        };

        match location {
            regalloc::Location::Reg(r) => *r,
            regalloc::Location::Stack => {
                todo!("no stack handling yet, maybe this should be a stack slot?")
            }
        }
    }

    /// Compile a list of ir functions to bytecode instructions
    pub fn compile(&mut self, ir: &[Func<'cc>]) -> Result<(), PgError> {
        for func in ir {
            self.cc(func)?;
        }
        Ok(())
    }

    fn cc(&mut self, fun: &Func<'cc>) -> Result<(), PgError> {
        // since we have a ssa based ir, we use our register allocator in a function local way and
        // spill any register usage >= 64 on the vm stack, this should be very fast for the general
        // usage and extensible enough for extreme niche usecases requiring more than 64 alive
        // values at the same time

        self.regalloc = Ralloc::new(&fun.live_set);
        crate::trace!(
            "[bc] Computed ralloc map for `{}`: {:#?}",
            fun.name,
            &self.regalloc.map
        );
        let pc = self.buf.len();
        let f: BcFunc<'cc> = BcFunc { pc, name: fun.name };

        // binding the id of a function to its context
        self.functions.insert(fun.id, f);

        for block in &fun.blocks {
            if block.tombstone {
                continue;
            }

            self.block_map.insert(block.id, self.buf.len() as u16);

            for (i, instruction) in block.instructions.iter().enumerate() {
                self.instr(fun, i as u32, instruction);
            }

            self.term(fun, block.term.as_ref());
        }

        crate::trace!(
            "[bc] compiled `{}` (size={})",
            fun.name,
            self.buf.len() - pc
        );

        Ok(())
    }

    fn term(&mut self, fun: &Func<'cc>, t: Option<&ir::Terminator>) {
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
                let target = &fun.blocks.get(id.0 as usize).unwrap();
                for (i, param) in params.iter().enumerate() {
                    let ir::Id(src) = param;
                    let ir::Id(dst) = target.params[i];

                    if *src == dst {
                        continue;
                    }

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
                yes: (yes, yes_params),
                no: (no, no_params),
                ..
            } => {
                let target = &fun.blocks.get(yes.0 as usize).unwrap();
                for (i, param) in yes_params.iter().enumerate() {
                    let ir::Id(src) = param;
                    let ir::Id(dst) = target.params[i];

                    if *src == dst {
                        continue;
                    }

                    self.emit(Op::Mov {
                        dst: dst as u8,
                        src: *src as u8,
                    });
                }

                let ir::Id(cond) = cond;
                self.emit(Op::JmpF {
                    cond: *cond as u8,
                    target: yes.0 as u16,
                });

                let target = &fun.blocks.get(no.0 as usize).unwrap();
                for (i, param) in no_params.iter().enumerate() {
                    let ir::Id(src) = param;
                    let ir::Id(dst) = target.params[i];

                    if *src == dst {
                        continue;
                    }

                    self.emit(Op::Mov {
                        dst: dst as u8,
                        src: *src as u8,
                    });
                }

                self.emit(Op::Jmp {
                    target: no.0 as u16,
                });
            }
        }
    }

    fn instr(&mut self, fun: &Func<'cc>, pos: u32, i: &ir::Instr<'cc>) {
        match i {
            ir::Instr::Cast {
                dst: TypeId { id, ty },
                from,
            } => {
                let dst = self.ensure_register(*id);
                let src = self.ensure_register(*from);
                let op = match ty {
                    ptype::Type::Bool => Op::CastToBool { dst, src },
                    ptype::Type::Int => Op::CastToInt { dst, src },
                    ptype::Type::Double => Op::CastToDouble { dst, src },
                    _ => unreachable!("Not a valid cast, see typecheck::Typechecker::cast"),
                };
                self.emit(op);
            }
            ir::Instr::LoadConst { dst, value } => {
                let dst = self.ensure_register(dst.id);
                if let Const::Int(i) = value
                    && *i < i32::MAX as i64
                {
                    self.emit(Op::LoadI {
                        dst,
                        value: *i as i32,
                    });
                } else {
                    let idx = self.intern(*value);
                    self.emit(Op::LoadG { dst, idx });
                }
            }
            ir::Instr::Call { dst, func, args } => {
                let Some(func) = self.functions.get(func) else {
                    unreachable!();
                };

                // [ live registers ]
                //         |
                //         v
                //    (spill live values)
                //         |
                //         v
                //    (move args → r0..rN)
                //         |
                //         v
                //         call
                //         |
                //         v
                //    r0 = return value
                //         |
                //         v
                //    (reload spilled values)
                //         |
                //         v
                // [ continue ]

                let pc = func.pc;
                let mut r_to_spil = vec![];
                for (v, (def, last_use)) in &fun.live_set {
                    // the value is defined before the call and used after the call, thus must be
                    // spilled
                    if def < &pos && &pos < last_use {
                        crate::trace!(
                            "[bc] spilled r{} at call_idx={};def={};last_use={}",
                            v,
                            pos,
                            def,
                            last_use
                        );
                        r_to_spil.push(v);
                        self.emit(Op::Push { src: *v as u8 });
                    }
                }

                for (i, &ir::Id(arg)) in args.iter().enumerate() {
                    let (dst, src) = (i as u8, arg as u8);
                    if dst != src {
                        self.emit(Op::Mov { dst, src });
                    }
                }

                let TypeId {
                    id: ir::Id(dst), ..
                } = dst;
                self.emit(Op::Call { func: pc as u32 });
                self.emit(Op::Mov {
                    dst: *dst as u8,
                    src: 0,
                });

                for r in r_to_spil {
                    self.emit(Op::Pop { dst: *r as u8 });
                }
            }
            ir::Instr::Tail { dst, func, args } => {
                let Some(func) = self.functions.get(func) else {
                    unreachable!();
                };

                let pc = func.pc;
                for (i, &ir::Id(arg)) in args.iter().enumerate() {
                    let (dst, src) = (i as u8, arg as u8);
                    if dst != src {
                        self.emit(Op::Mov { dst, src });
                    }
                }

                self.emit(Op::Tail { func: pc as u32 });
            }
            ir::Instr::Sys {
                dst,
                path,
                func,
                args,
            } => {
                let idx = self.std_fns.intern(func.ptr);
                for (i, &ir::Id(arg)) in args.iter().enumerate() {
                    let (dst, src) = (i as u8, arg as u8);
                    if dst != src {
                        self.emit(Op::Mov { dst, src });
                    }
                }

                let TypeId {
                    id: ir::Id(dst), ..
                } = dst;
                self.emit(Op::Sys { idx: idx as u16 });
                self.emit(Op::Mov {
                    dst: *dst as u8,
                    src: 0,
                });
            }
            ir::Instr::Noop {} => {}
            ir::Instr::Bin { op, dst, lhs, rhs } => {
                let dst = self.ensure_register(dst.id);
                let lhs = self.ensure_register(*lhs);
                let rhs = self.ensure_register(*rhs);

                macro_rules! emit_bins {
                    ($($name:ident),*) => {
                        match op {
                            $(
                                ir::BinOp::$name => {
                                    Op::$name {
                                        dst,
                                        lhs,
                                        rhs,
                                    }
                                },
                            )*
                        }
                    };
                }

                self.emit(emit_bins! {
                    IAdd, ISub, IMul, IDiv, ILt, IGt, IEq,
                    DAdd, DSub, DMul, DDiv, DLt, DGt,
                    BEq
                });
            }
        };
    }

    pub fn finalize(mut self, config: &'cc Config) -> Vm<'cc> {
        let mut v = Vm::new(config);
        v.pc = self
            .functions
            .get(&ir::Id(0))
            .map(|n| n.pc)
            .unwrap_or_default();

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
        v.globals = self.globals.into_vec_fn(Value::from);
        v.strings = self.strings.into_vec_fn(|s| s.to_owned().into_boxed_str());
        v.syscalls = self.std_fns.into_vec();
        v
    }

    /// map pc's to function definitions
    pub fn function_table(&self) -> HashMap<usize, String> {
        self.functions
            .values()
            .map(|f| (f.pc, f.name.to_string()))
            .collect()
    }
}

impl<'cc> Default for Cc<'cc> {
    fn default() -> Self {
        Self::new()
    }
}
