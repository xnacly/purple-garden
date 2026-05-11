use std::collections::HashMap;

pub mod dis;
mod intern;
mod regalloc;

use crate::{
    bc::{intern::Interner, regalloc::Ralloc},
    config::{self, Config},
    err::PgError,
    ir::{self, Const, Func, Id, TypeId, ptype},
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
    pub fn compile(&mut self, conf: &config::Config, ir: &[Func<'cc>]) -> Result<(), PgError> {
        for func in ir {
            if conf.liveness {
                let intervals = func.live_set();
                let mut entries: Vec<_> = intervals.iter().collect();
                entries.sort_by_key(|(id, _)| *id);
                println!(
                    "{}",
                    entries
                        .into_iter()
                        .map(|(id, (def, last_use))| format!("{id}: ({def},{last_use})\n"))
                        .collect::<String>()
                )
            }
            self.cc(func)?;
        }
        Ok(())
    }

    fn cc(&mut self, fun: &Func<'cc>) -> Result<(), PgError> {
        let live_set = fun.live_set();
        self.regalloc = Ralloc::new(&live_set);
        crate::trace!(
            "[bc::Cc::cc][{}] regalloc map: {:#?}",
            fun.name,
            &self.regalloc.map
        );
        let pc = self.buf.len();
        let f: BcFunc<'cc> = BcFunc { pc, name: fun.name };

        // binding the id of a function to its context
        self.functions.insert(fun.id, f);

        // block_map is keyed by ir::Id, but ir block ids restart at 0 in
        // every function, thus we need to clear it on each new function
        self.block_map.clear();

        // pos must mirror the global position counter used by Func::live_set:
        // +1 for the block's params row, +1 per instruction, +1 for the
        // terminator.
        //
        // The caller save spill check around call uses pos to idx into (def, last_use) intervals
        // from live_set.
        let mut pos: u32 = 0;
        for block in &fun.blocks {
            if block.tombstone {
                continue;
            }

            self.block_map.insert(block.id, self.buf.len() as u16);

            pos += 1; // block params row
            for instruction in &block.instructions {
                self.instr(&live_set, pos, instruction);
                pos += 1;
            }

            self.term(fun, block.term.as_ref());
            pos += 1; // terminator row
        }

        for i in pc..self.buf.len() {
            self.buf[i] = match self.buf[i] {
                Op::JmpF { cond, target } => Op::JmpF {
                    cond,
                    target: *self.block_map.get(&ir::Id(target as u32)).unwrap(),
                },
                Op::Jmp { target } => Op::Jmp {
                    target: *self.block_map.get(&ir::Id(target as u32)).unwrap(),
                },
                other => other,
            };
        }

        crate::trace!("[bc::Cc::cc][{}] size={}", fun.name, self.buf.len() - pc);

        Ok(())
    }

    /// Move `args[i]` into the i argument register (`r0..rN`) for a call
    /// or tail.
    ///
    /// Parallel-move: emit a direct Mov for any pending pair whose
    /// dst isn't another pending move's src. When all that remains is one
    /// or more cycles (e.g. swap r0,r1), fall back to push and pop for those
    /// leftovers only.
    fn emit_arg_shuffle(&mut self, args: &[Id]) {
        let mut todo: Vec<(u8, u8)> = args
            .iter()
            .enumerate()
            .map(|(i, a)| (self.ensure_register(*a), i as u8))
            .filter(|(s, d)| s != d)
            .collect();

        'outer: loop {
            if todo.is_empty() {
                return;
            }
            for i in 0..todo.len() {
                let (src, dst) = todo[i];
                if !todo.iter().any(|(s, _)| *s == dst) {
                    self.emit(Op::Mov { dst, src });
                    todo.swap_remove(i);
                    continue 'outer;
                }
            }

            // Remaining moves form one or more cycles; break them via the
            // spill stack. Push all sources, then pop into dsts in reverse
            // so the LIFO order lines up.
            for &(src, _) in &todo {
                self.emit(Op::Push { src });
            }
            for &(_, dst) in todo.iter().rev() {
                self.emit(Op::Pop { dst });
            }
            return;
        }
    }

    fn term(&mut self, fun: &Func<'cc>, t: Option<&ir::Terminator>) {
        let Some(term) = t else {
            return;
        };

        match term {
            ir::Terminator::Return(id) => {
                if let Some(src_id) = id {
                    let src = self.ensure_register(*src_id);
                    self.emit(Op::Mov { dst: 0, src });
                }
                self.emit(Op::Ret);
            }
            ir::Terminator::Jump { id, params } => {
                let target = &fun.blocks.get(id.0 as usize).unwrap();

                for (i, param) in params.iter().enumerate() {
                    let src = self.ensure_register(*param);
                    let dst = self.ensure_register(target.params[i]);

                    if src == dst {
                        continue;
                    }
                    self.emit(Op::Mov { dst, src });
                }

                let ir::Id(id) = id;
                // this gets patched in Cc::finalize after all bytecode is emitted
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
                    let src = self.ensure_register(*param);
                    let dst = self.ensure_register(target.params[i]);

                    if src == dst {
                        continue;
                    }
                    self.emit(Op::Mov { dst, src });
                }

                let cond = self.ensure_register(*cond);
                self.emit(Op::JmpF {
                    cond,
                    target: yes.0 as u16,
                });

                let target = &fun.blocks.get(no.0 as usize).unwrap();
                for (i, param) in no_params.iter().enumerate() {
                    let src = self.ensure_register(*param);
                    let dst = self.ensure_register(target.params[i]);

                    if src == dst {
                        continue;
                    }
                    self.emit(Op::Mov { dst, src });
                }

                self.emit(Op::Jmp {
                    target: no.0 as u16,
                });
            }
            ir::Terminator::Tail { func, args } => {
                let Some(func) = self.functions.get(func) else {
                    unreachable!();
                };

                let pc = func.pc;
                self.emit_arg_shuffle(args);

                self.emit(Op::Tail { func: pc as u32 });
            }
        }
    }

    fn instr(&mut self, live_set: &HashMap<Id, (Id, Id)>, pos: u32, i: &ir::Instr<'cc>) {
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

                let pc = func.pc;

                let mut alive_after_call_spill = vec![];
                for (Id(v), (Id(def), Id(last_use))) in live_set {
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
                        let Some(regalloc::Location::Reg(src)) = self.regalloc.map.get(v) else {
                            unreachable!();
                        };

                        let src = *src;
                        alive_after_call_spill.push(src);
                        self.emit(Op::Push { src });
                    }
                }

                self.emit_arg_shuffle(args);

                let dst = self.ensure_register(dst.id);
                self.emit(Op::Call { func: pc as u32 });
                self.emit(Op::Mov { dst, src: 0 });
                for dst in alive_after_call_spill.iter().rev() {
                    self.emit(Op::Pop { dst: *dst });
                }
            }
            ir::Instr::Sys {
                dst, func, args, ..
            } => {
                let idx = self.std_fns.intern(func.ptr);

                // Syscall calling convention: only r0 is clobbered by the
                // syscall body (it gets the result). The shuffle additionally
                // writes r0..r{argcount-1}. Combined clobber range is
                // r0..max(argcount, 1). Spill only alive-across values whose
                // register falls in that range; everything else is preserved
                // by the convention.
                let clobber_end = args.len().max(1) as u8;
                let mut alive_across_spill = vec![];
                for (Id(v), (Id(def), Id(last_use))) in live_set {
                    if def < &pos && &pos < last_use {
                        let Some(regalloc::Location::Reg(src)) = self.regalloc.map.get(v) else {
                            unreachable!();
                        };
                        if *src < clobber_end {
                            alive_across_spill.push(*src);
                            self.emit(Op::Push { src: *src });
                        }
                    }
                }

                self.emit_arg_shuffle(args);

                let dst = self.ensure_register(dst.id);
                self.emit(Op::Sys { idx: idx as u16 });
                self.emit(Op::Mov { dst, src: 0 });
                for dst in alive_across_spill.into_iter().rev() {
                    self.emit(Op::Pop { dst });
                }
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

    // PERF: i have no idea how this impacts the compilation cost, but its better for runtime, since
    // peephole now no longer leaves artifacts behind inflicting dispatch cost

    /// Strip [Op::Nop]s left behind by [opt::bc] and patch every absolute pc
    /// (jump targets, call/tail targets, function entry pcs in
    /// [Cc::functions]) through an old->new pc remap. Must run after all
    /// peephole passes since indices shift here.
    pub fn compact_nops(&mut self) {
        let bc = &mut self.buf;
        if bc.is_empty() {
            return;
        }

        // Skip the whole pass when peephole produced nothing to compact.
        // Avoids the remap allocation and second walk in the common no-op case.
        if !bc.iter().any(|op| matches!(op, Op::Nop)) {
            return;
        }

        // bc.len() fits in u16 since Jmp.target is u16; halve the remap
        // table's cache footprint vs Vec<u32>.
        let mut old_to_new = vec![0u16; bc.len() + 1];
        let mut new_pc: u16 = 0;
        for (i, op) in bc.iter().enumerate() {
            old_to_new[i] = new_pc;
            if !matches!(op, Op::Nop) {
                new_pc += 1;
            }
        }
        // sentinel so a pc that points one past the end remaps cleanly
        old_to_new[bc.len()] = new_pc;

        let mut w = 0;
        for r in 0..bc.len() {
            let mut op = bc[r];
            match &mut op {
                Op::Jmp { target } => {
                    *target = old_to_new[*target as usize];
                }
                Op::JmpF { target, .. } => {
                    *target = old_to_new[*target as usize];
                }
                Op::Call { func } => {
                    *func = old_to_new[*func as usize] as u32;
                }
                Op::Tail { func } => {
                    *func = old_to_new[*func as usize] as u32;
                }
                _ => {}
            }
            if !matches!(op, Op::Nop) {
                bc[w] = op;
                w += 1;
            }
        }
        bc.truncate(w);

        for f in self.functions.values_mut() {
            f.pc = old_to_new[f.pc] as usize;
        }
    }

    pub fn finalize(self, config: &'cc Config) -> Vm<'cc> {
        let mut v = Vm::new(config);
        v.pc = self
            .functions
            .get(&ir::Id(0))
            .map(|n| n.pc)
            .unwrap_or_default();

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
