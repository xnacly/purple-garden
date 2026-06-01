use std::collections::HashMap;
use std::fmt::Write as _;

pub mod dis;
mod intern;
mod regalloc;

use crate::{intern::Interner, regalloc::Ralloc};
use purple_garden_ir::{self as ir, Func, Id, TypeId, constant::Const, ptype};
use purple_garden_runtime::{BuiltinFn, DebugInfo, Value, Vm, VmConfig, op::Op};
use purple_garden_shared::config::Config;

#[derive(Debug, Clone)]
pub enum CcFunc<'fun> {
    Bc {
        name: &'fun str,
        pc: usize,
    },
    /// `idx` is the syscall slot the JIT page was injected at; `insns` is the
    /// emitted native instruction list, owned here and kept for `-D` (the
    /// executable bytes live only in the page).
    Native {
        name: &'fun str,
        idx: u16,
        insns: Vec<purple_garden_jit::Insn>,
    },
}

impl<'fun> CcFunc<'fun> {
    #[must_use]
    pub fn name(&self) -> &'fun str {
        match self {
            Self::Bc { name, .. } | Self::Native { name, .. } => name,
        }
    }

    #[must_use]
    pub fn pc(&self) -> Option<usize> {
        match self {
            Self::Bc { pc, .. } => Some(*pc),
            Self::Native { .. } => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Cc<'cc> {
    pub buf: Vec<Op>,
    pub globals: Interner<Const<'cc>>,
    pub strings: Interner<&'cc str>,
    pub std_fns: Interner<BuiltinFn>,
    pub functions: HashMap<Id, CcFunc<'cc>>,
    /// `pc_to_span[pc]` is the byte offset into the source of the AST node
    /// that produced the op at `pc`. Threaded into Vm by `Cc::finalize` so
    /// runtime traps can be rendered with <file:line:col>. Parallel to
    /// `self.buf`.
    pub pc_to_span: Vec<u32>,
    /// `block_map[block_id]` is the absolute pc of that block's first op,
    /// after lowering. `u16::MAX` marks blocks that weren't emitted (e.g.,
    /// tombstoned blocks). Block ids are dense per-function so a Vec
    /// indexed by id beats a `HashMap` on both alloc cost and lookup speed.
    block_map: Vec<u16>,
    regalloc: Ralloc,
    /// Set once per IR Instr / Terminator before lowering, consumed by
    /// every `emit` call within that lowering. Saves threading a span
    /// argument through every `self.buf.push(op)` call site.
    cur_span: u32,
    /// Scratch buffers reused across [`Cc::cc`] invocations. `live_set` and
    /// `arg_hints` are taken out of `self` via [`std::mem::take`] for the
    /// duration of one compile, then put back with their grown capacity;
    /// so after the first few functions warm the buffers, subsequent
    /// `cc()` calls never re-allocate.
    live_set: Vec<(u32, u32)>,
    arg_hints: Vec<Option<u8>>,
    /// General-purpose `u8` scratch used by prologue/epilogue and call-site
    /// spills. Callers fill it, then immediately consume it via the
    /// `pack_push`/`pack_pop` free functions; it is never live across a
    /// call to another `Cc` method.
    scratch: Vec<u8>,
    /// Reusable `(src, dst)` buffer for the parallel-move resolver in
    /// [`Cc::emit_arg_shuffle`]. Replaces the per-call `todo: Vec<(u8,u8)>`
    /// allocation; taken out of `self` for the duration of each shuffle
    /// and restored on exit so the capacity survives across calls.
    scratch_pairs: Vec<(u8, u8)>,
    /// Callee-saved range for the function currently being compiled.
    /// Set at the top of [`Cc::cc`] and read by [`Cc::emit_arg_shuffle`]
    /// to find a free scratch register for cycle breaking.
    cur_lo: u8,
    cur_max_reg: u8,
    /// Reusable native-code buffer for the JIT, refilled per function. Empty
    /// and untouched when `--no-jit` is set.
    jit: purple_garden_jit::Jit,
}

/// Emit batched Push ops for `regs` in order, packing into Push3/Push2/Push.
/// Free function so callers can pass `&self.scratch` alongside `&mut self.buf`
/// without a whole-struct borrow.
fn pack_push(buf: &mut Vec<Op>, spans: &mut Vec<u32>, span: u32, regs: &[u8]) {
    let mut i = 0;
    while i + 3 <= regs.len() {
        buf.push(Op::Push3 {
            a: regs[i],
            b: regs[i + 1],
            c: regs[i + 2],
        });
        spans.push(span);
        i += 3;
    }
    if i + 2 <= regs.len() {
        buf.push(Op::Push2 {
            a: regs[i],
            b: regs[i + 1],
        });
        spans.push(span);
        i += 2;
    }
    if i < regs.len() {
        buf.push(Op::Push { src: regs[i] });
        spans.push(span);
    }
}

/// Emit batched Pop ops for `regs` in order, packing into Pop3/Pop2/Pop.
fn pack_pop(buf: &mut Vec<Op>, spans: &mut Vec<u32>, span: u32, regs: &[u8]) {
    let mut i = 0;
    while i + 3 <= regs.len() {
        buf.push(Op::Pop3 {
            a: regs[i],
            b: regs[i + 1],
            c: regs[i + 2],
        });
        spans.push(span);
        i += 3;
    }
    if i + 2 <= regs.len() {
        buf.push(Op::Pop2 {
            a: regs[i],
            b: regs[i + 1],
        });
        spans.push(span);
        i += 2;
    }
    if i < regs.len() {
        buf.push(Op::Pop { dst: regs[i] });
        spans.push(span);
    }
}

/// Like `pack_push` but sources are `pairs[i].0`; avoids collecting srcs into
/// a separate `Vec<u8>` when the caller already has a `Vec<(u8,u8)>`.
fn pack_push_pairs(buf: &mut Vec<Op>, spans: &mut Vec<u32>, span: u32, pairs: &[(u8, u8)]) {
    let mut i = 0;
    while i + 3 <= pairs.len() {
        buf.push(Op::Push3 {
            a: pairs[i].0,
            b: pairs[i + 1].0,
            c: pairs[i + 2].0,
        });
        spans.push(span);
        i += 3;
    }
    if i + 2 <= pairs.len() {
        buf.push(Op::Push2 {
            a: pairs[i].0,
            b: pairs[i + 1].0,
        });
        spans.push(span);
        i += 2;
    }
    if i < pairs.len() {
        buf.push(Op::Push { src: pairs[i].0 });
        spans.push(span);
    }
}

/// Like `pack_pop` but dsts are `pairs[n-1-i].1` (reversed); avoids collecting
/// into a separate `Vec<u8>`.
fn pack_pop_pairs_rev(buf: &mut Vec<Op>, spans: &mut Vec<u32>, span: u32, pairs: &[(u8, u8)]) {
    let n = pairs.len();
    let mut i = 0;
    while i + 3 <= n {
        let j = n - 1 - i;
        buf.push(Op::Pop3 {
            a: pairs[j].1,
            b: pairs[j - 1].1,
            c: pairs[j - 2].1,
        });
        spans.push(span);
        i += 3;
    }
    if i + 2 <= n {
        let j = n - 1 - i;
        buf.push(Op::Pop2 {
            a: pairs[j].1,
            b: pairs[j - 1].1,
        });
        spans.push(span);
        i += 2;
    }
    if i < n {
        buf.push(Op::Pop {
            dst: pairs[n - 1 - i].1,
        });
        spans.push(span);
    }
}

impl<'cc> Cc<'cc> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(64),
            pc_to_span: Vec::with_capacity(64),
            globals: Interner::new(),
            strings: Interner::new(),
            std_fns: Interner::new(),
            functions: HashMap::new(),
            block_map: Vec::new(),
            regalloc: Ralloc::default(),
            cur_span: 0,
            live_set: Vec::new(),
            arg_hints: Vec::new(),
            scratch: Vec::new(),
            scratch_pairs: Vec::new(),
            cur_lo: 0,
            cur_max_reg: 0,
            jit: purple_garden_jit::Jit::new(),
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
        self.pc_to_span.push(self.cur_span);
        pc
    }

    fn ensure_register(&self, Id(id): Id) -> u8 {
        match self.regalloc.map.get(id as usize) {
            Some(regalloc::Location::Reg(r)) => *r,
            Some(regalloc::Location::Stack) => {
                todo!("no stack handling yet, maybe this should be a stack slot?")
            }
            Some(regalloc::Location::Unassigned) | None => unreachable!(
                "Attempted a register alloc lookup for a not defined ssa virtual register %v{}",
                id
            ),
        }
    }

    /// Compile a list of ir functions to bytecode instructions
    pub fn compile(
        &mut self,
        config: &Config,
        ir: &[Func<'cc>],
        pkg_fns: &HashMap<&str, HashMap<&str, BuiltinFn>>,
    ) -> Result<Vec<purple_garden_jit::JitFn>, String> {
        let mut native_pages: Option<Vec<purple_garden_jit::JitFn>> =
            (!config.no_jit).then(Vec::new);

        for func in ir {
            if config.liveness {
                let mut intervals = Vec::new();
                func.live_set_into(&mut intervals);
                let mut out = String::new();
                for (id, &(def, last_use)) in intervals.iter().enumerate() {
                    if def == u32::MAX {
                        continue;
                    }
                    writeln!(out, "{id}: ({def},{last_use})").unwrap();
                }
                println!("{out}");
            }
            self.cc(func, pkg_fns, native_pages.as_mut())?;
        }

        Ok(native_pages.unwrap_or_default())
    }

    fn cc(
        &mut self,
        fun: &Func<'cc>,
        pkg_fns: &HashMap<&str, HashMap<&str, BuiltinFn>>,
        native: Option<&mut Vec<purple_garden_jit::JitFn>>,
    ) -> Result<(), String> {
        // Take the reusable scratch buffers out of self so we can hold an
        // immutable borrow of `live_set` across calls to `&mut self`
        // helpers (e.g. `self.instr`) without tripping the borrow checker.
        //
        // Put them back before returning so the next cc() reuses the same
        // capacity; after a few warm functions, this path is alloc-free.
        let mut live_set = std::mem::take(&mut self.live_set);
        let mut arg_hints = std::mem::take(&mut self.arg_hints);
        fun.live_set_into(&mut live_set);
        fun.arg_hints_into(&mut arg_hints);
        self.regalloc.rebuild(&live_set, &arg_hints);

        // The JIT reuses the register assignment computed above instead of
        // recomputing its own. The entry function stays bytecode-dispatched:
        // the VM enters it directly, never through a Sys, so it can't go native.
        let native = match native {
            Some(_) if fun.id == ir::Id(0) => {
                purple_garden_shared::trace!(
                    "[bc::Cc::cc][{}] native skipped: entry must remain bytecode-dispatched",
                    fun.name
                );
                None
            }
            other => other,
        };
        if let Some(native) = native
            && self.try_compile_native(fun, native)?
        {
            purple_garden_shared::trace!("[bc::Cc::cc][{}] native", fun.name);
            self.live_set = live_set;
            self.arg_hints = arg_hints;
            return Ok(());
        }

        // binding the id of a function to its context
        let pc = self.buf.len();
        self.functions
            .insert(fun.id, CcFunc::Bc { pc, name: fun.name });

        let max_reg = self.regalloc.max_reg();
        // lo = first callee-saved register: skip r0..r{nparams-1} (arg zone) and
        // always skip r0 (return slot). So lo = max(nparams, 1).
        let nparams = fun
            .blocks
            .iter()
            .find(|b| !b.tombstone)
            .map(|b| fun.params(b.params).len())
            .unwrap_or(0) as u8;
        let is_root = fun.id == ir::Id(0);
        let lo = if is_root {
            max_reg.saturating_add(1)
        } else {
            nparams.max(1)
        };
        self.cur_lo = lo;
        self.cur_max_reg = max_reg;
        self.cur_span = fun.span;

        // block_map is indexed by ir block id, and block ids restart at 0 per function; size it to
        // the current function's block count and fill with the u16::MAX sentinel for
        // tombstoned/unemitted blocks.
        self.block_map.clear();
        self.block_map.resize(fun.blocks.len(), u16::MAX);

        // Callee-saved prologue: push r{lo}..r{max_reg} before the first block.
        // The matching epilogue is emitted before every Op::Ret and Op::Tail.
        self.emit_prologue(lo, max_reg);

        // Two slots per event (early then late), in lockstep with
        // Func::live_set_into. pos points at the early slot; the
        // call-site spill check compares last_use against it.
        let mut pos: u32 = 0;
        for (idx, block) in fun.blocks.iter().enumerate() {
            if block.tombstone {
                continue;
            }

            self.block_map[block.id.0 as usize] = self.buf.len() as u16;

            pos += 2; // block params row
            for instruction in &block.instructions {
                self.cur_span = instruction.span();
                self.instr(&live_set, pos, instruction, pkg_fns);
                pos += 2;
            }

            if let Some(term) = block.term.as_ref() {
                self.cur_span = term.span();
            }
            // The next emitted block; the Branch lowering uses it to fuse
            // JmpT yes + Jmp no into a single JmpF when yes is the
            // fall-through.
            let next_block = fun.blocks[idx + 1..]
                .iter()
                .find(|b| !b.tombstone)
                .map(|b| b.id);
            self.term(fun, block.term.as_ref(), next_block, lo, max_reg);
            pos += 2; // terminator row
        }

        for i in pc..self.buf.len() {
            self.buf[i] = match self.buf[i] {
                Op::JmpT { cond, target } => Op::JmpT {
                    cond,
                    target: self.block_map[target as usize],
                },
                Op::JmpF { cond, target } => Op::JmpF {
                    cond,
                    target: self.block_map[target as usize],
                },
                Op::Jmp { target } => Op::Jmp {
                    target: self.block_map[target as usize],
                },
                other => other,
            };
        }

        purple_garden_shared::trace!("[bc::Cc::cc][{}] size={}", fun.name, self.buf.len() - pc);

        // Hand the scratch buffers back to self with their grown capacity.
        self.live_set = live_set;
        self.arg_hints = arg_hints;

        Ok(())
    }

    fn try_compile_native(
        &mut self,
        fun: &Func<'cc>,
        pages: &mut Vec<purple_garden_jit::JitFn>,
    ) -> Result<bool, String> {
        let Some(insns) = self.jit.compile_func(fun) else {
            purple_garden_shared::trace!("[bc::Cc::cc] native skipped function {}", fun.name);
            return Ok(false);
        };

        // The page copies the bytes; we move the instruction list onto the
        // CcFunc (single owner, no duplicate byte blob) for `-D`.
        let jit = purple_garden_jit::JitFn::new(self.jit.code())
            .map_err(|e| format!("native code allocation failed: {e}"))?;
        let idx = self.std_fns.intern(jit.entry()) as u16;
        self.functions.insert(
            fun.id,
            CcFunc::Native {
                name: fun.name,
                idx,
                insns,
            },
        );
        pages.push(jit);
        Ok(true)
    }

    /// Move `args[i]` into the i-th argument register (`r0..r{N-1}`) for a call
    /// or tail. `r0` is both the first argument and the return-value slot.
    ///
    /// Parallel-move: emit a direct Mov for any pending pair whose
    /// dst isn't another pending move's src. When all that remains is one
    /// or more cycles (e.g. swap r0,r1), fall back to push and pop for those
    /// leftovers only.
    fn emit_arg_shuffle(&mut self, args: &[Id]) {
        let mut todo = std::mem::take(&mut self.scratch_pairs);
        todo.clear();
        todo.extend(
            args.iter()
                .enumerate()
                .map(|(i, a)| (self.ensure_register(*a), i as u8))
                .filter(|(s, d)| s != d),
        );

        'outer: loop {
            if todo.is_empty() {
                break;
            }
            for i in 0..todo.len() {
                let (src, dst) = todo[i];
                if !todo.iter().any(|(s, _)| *s == dst) {
                    self.emit(Op::Mov { dst, src });
                    todo.swap_remove(i);
                    continue 'outer;
                }
            }

            // All remaining moves form cycles. Find a callee-saved register
            // that isn't a src or dst in any pending move and use it to break
            // one cycle at a time without touching the spill stack.
            let scratch = (self.cur_lo..=self.cur_max_reg)
                .find(|&r| !todo.iter().any(|(s, d)| *s == r || *d == r));

            if let Some(scratch) = scratch {
                // Break the first cycle: save its head into scratch, walk
                // the chain (find whose dst == the just-freed register),
                // emit each Mov in turn, then close by writing scratch into
                // the head's destination.
                let (start_src, start_dst) = todo.swap_remove(0);
                self.emit(Op::Mov {
                    dst: scratch,
                    src: start_src,
                });
                let mut cur_freed = start_src;
                loop {
                    if let Some(idx) = todo.iter().position(|(_, d)| *d == cur_freed) {
                        let (src, dst) = todo.swap_remove(idx);
                        self.emit(Op::Mov { dst, src });
                        cur_freed = src;
                    } else {
                        break;
                    }
                }
                self.emit(Op::Mov {
                    dst: start_dst,
                    src: scratch,
                });
                // Loop back to handle any remaining cycles.
            } else {
                // No free register available; fall back to spill stack.
                pack_push_pairs(&mut self.buf, &mut self.pc_to_span, self.cur_span, &todo);
                pack_pop_pairs_rev(&mut self.buf, &mut self.pc_to_span, self.cur_span, &todo);
                break;
            }
        }

        self.scratch_pairs = todo;
    }

    /// Push r{lo}..r{max_reg} onto the spill stack (callee-saved prologue). `lo = max(nparams, 1)`;
    /// skips the arg zone (r0..r{nparams-1}) and r0 (the return slot), which are never
    /// callee-saved.
    fn emit_prologue(&mut self, lo: u8, max_reg: u8) {
        if lo > max_reg {
            return;
        }
        self.scratch.clear();
        self.scratch.extend(lo..=max_reg);
        pack_push(
            &mut self.buf,
            &mut self.pc_to_span,
            self.cur_span,
            &self.scratch,
        );
    }

    /// Pop r{max_reg}..r{lo} from the spill stack (callee-saved epilogue).
    /// Must mirror emit_prologue exactly (LIFO order).
    fn emit_epilogue(&mut self, lo: u8, max_reg: u8) {
        if lo > max_reg {
            return;
        }
        self.scratch.clear();
        self.scratch.extend((lo..=max_reg).rev());
        pack_pop(
            &mut self.buf,
            &mut self.pc_to_span,
            self.cur_span,
            &self.scratch,
        );
    }

    fn term(
        &mut self,
        fun: &Func<'cc>,
        t: Option<&ir::Terminator>,
        next_block: Option<ir::Id>,
        lo: u8,
        max_reg: u8,
    ) {
        let Some(term) = t else {
            return;
        };

        match term {
            ir::Terminator::Return { value: id, .. } => {
                if let Some(src_id) = id {
                    let src = self.ensure_register(*src_id);
                    self.emit(Op::Mov { dst: 0, src });
                }
                self.emit_epilogue(lo, max_reg);
                self.emit(Op::Ret);
            }
            ir::Terminator::Jump { id, params, .. } => {
                let target = &fun.blocks.get(id.0 as usize).unwrap();
                let src_params = fun.params(*params);
                let dst_params = fun.params(target.params);

                for (i, &param) in src_params.iter().enumerate() {
                    let src = self.ensure_register(param);
                    let dst = self.ensure_register(dst_params[i]);

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
                let yes_target = &fun.blocks.get(yes.0 as usize).unwrap();
                let yes_src = fun.params(*yes_params);
                let yes_dst = fun.params(yes_target.params);

                let no_target = &fun.blocks.get(no.0 as usize).unwrap();
                let no_src = fun.params(*no_params);
                let no_dst = fun.params(no_target.params);

                // yes_movs run unconditionally; regalloc guarantees their
                // dst regs are safe on the no-path.
                for (i, &param) in yes_src.iter().enumerate() {
                    let src = self.ensure_register(param);
                    let dst = self.ensure_register(yes_dst[i]);
                    if src == dst {
                        continue;
                    }
                    self.emit(Op::Mov { dst, src });
                }

                let cond_reg = self.ensure_register(*cond);

                // Fuse JmpT yes + Jmp no into a single JmpF when yes is
                // the fall-through AND no-side shuffle is empty. With
                // non-empty no_movs there is nowhere left to put them
                // once the unconditional Jmp is dropped. The no==next
                // case is already handled by the jmp_next peephole.
                let no_movs_empty = no_src
                    .iter()
                    .zip(no_dst)
                    .all(|(&s, &d)| self.ensure_register(s) == self.ensure_register(d));

                if Some(*yes) == next_block && no_movs_empty {
                    self.emit(Op::JmpF {
                        cond: cond_reg,
                        target: no.0 as u16,
                    });
                } else {
                    self.emit(Op::JmpT {
                        cond: cond_reg,
                        target: yes.0 as u16,
                    });
                    for (i, &param) in no_src.iter().enumerate() {
                        let src = self.ensure_register(param);
                        let dst = self.ensure_register(no_dst[i]);
                        if src == dst {
                            continue;
                        }
                        self.emit(Op::Mov { dst, src });
                    }
                    self.emit(Op::Jmp {
                        target: no.0 as u16,
                    });
                }
            }
            ir::Terminator::Tail { func, args, .. } => {
                let Some(func) = self.functions.get(func) else {
                    unreachable!();
                };

                // Clone to release the `self.functions` borrow before the emit
                // calls below take `&mut self`.
                let func = func.clone();

                // Arg shuffle first (reads computed values from callee-saved regs),
                // then epilogue (restores r{lo}..r{max_reg}, which is above the arg
                // zone r0..r{nparams-1}; no overlap for ta <= nparams, the common case).
                self.emit_arg_shuffle(args);
                self.emit_epilogue(lo, max_reg);

                match func {
                    CcFunc::Bc { pc, .. } => self.emit(Op::Tail { func: pc as u32 }),
                    CcFunc::Native { idx, .. } => {
                        self.emit(Op::Sys { idx });
                        self.emit(Op::Ret)
                    }
                };
            }
        }
    }

    fn instr(
        &mut self,
        live_set: &[(u32, u32)],
        pos: u32,
        i: &ir::Instr<'cc>,
        pkg_fns: &HashMap<&str, HashMap<&str, BuiltinFn>>,
    ) {
        match i {
            ir::Instr::Cast {
                dst: TypeId { id, ty: dst_ty },
                from:
                    TypeId {
                        id: src_id,
                        ty: src_ty,
                    },
                ..
            } => {
                use ptype::Type::{Bool, Double, Int};
                let dst = self.ensure_register(*id);
                let src = self.ensure_register(*src_id);
                let op = match (src_ty, dst_ty) {
                    (Int, Double) => Op::CastToDouble { dst, src },
                    (Double, Int) => Op::CastToInt { dst, src },
                    (Int, Bool) => Op::CastToBool { dst, src },
                    // Bool and Int share the same u64 representation.
                    (Bool, Int) => Op::Mov { dst, src },
                    _ => unreachable!("Not a valid cast, see typecheck::Typechecker::cast"),
                };
                self.emit(op);
            }
            ir::Instr::LoadConst { dst, value, .. } => {
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
            ir::Instr::Call {
                dst, func, args, ..
            } => {
                let Some(func) = self.functions.get(func) else {
                    unreachable!();
                };
                // Clone to release the `self.functions` borrow before the emit
                // calls below take `&mut self`.
                let func = func.clone();

                // Callee-saved convention: the callee's prologue preserves r1..r{max_reg_callee}.
                // The caller only needs to spill live values in r0..r{clobber_end-1},
                // the arg-shuffle zone. r{clobber_end}+ are untouched from the caller's view.
                let clobber_end = args.len().max(1) as u8;
                self.scratch.clear();
                for (v, &(def, last_use)) in live_set.iter().enumerate() {
                    if def == u32::MAX {
                        continue;
                    }
                    if def < pos && pos < last_use {
                        let regalloc::Location::Reg(src) = self.regalloc.map[v] else {
                            unreachable!();
                        };
                        if src < clobber_end {
                            purple_garden_shared::trace!(
                                "[bc] spilled r{} at call_idx={};def={};last_use={}",
                                src,
                                pos,
                                def,
                                last_use
                            );
                            self.scratch.push(src);
                        }
                    }
                }
                pack_push(
                    &mut self.buf,
                    &mut self.pc_to_span,
                    self.cur_span,
                    &self.scratch,
                );

                self.emit_arg_shuffle(args);

                let dst = self.ensure_register(dst.id);
                match func {
                    CcFunc::Bc { pc, .. } => self.emit(Op::Call { func: pc as u32 }),
                    CcFunc::Native { idx, .. } => self.emit(Op::Sys { idx }),
                };
                self.emit(Op::Mov { dst, src: 0 });
                self.scratch.reverse();
                pack_pop(
                    &mut self.buf,
                    &mut self.pc_to_span,
                    self.cur_span,
                    &self.scratch,
                );
            }
            ir::Instr::Sys {
                dst,
                path,
                name,
                args,
                ..
            } => {
                let ptr = pkg_fns[path][name];
                let idx = self.std_fns.intern(ptr);

                // Syscall convention: shuffle writes r0..r{argcount-1}, syscall
                // body writes r0 (return slot). Clobber range is r0..r{clobber_end-1}
                // (clobber_end = args.len().max(1)). Builtins never touch higher
                // registers, so only spill alive-across values inside that range.
                let clobber_end = args.len().max(1) as u8;
                self.scratch.clear();
                for (v, &(def, last_use)) in live_set.iter().enumerate() {
                    if def == u32::MAX {
                        continue;
                    }
                    if def < pos && pos < last_use {
                        let regalloc::Location::Reg(src) = self.regalloc.map[v] else {
                            unreachable!();
                        };
                        if src < clobber_end {
                            self.scratch.push(src);
                        }
                    }
                }
                pack_push(
                    &mut self.buf,
                    &mut self.pc_to_span,
                    self.cur_span,
                    &self.scratch,
                );

                self.emit_arg_shuffle(args);

                let dst = self.ensure_register(dst.id);
                self.emit(Op::Sys { idx: idx as u16 });
                self.emit(Op::Mov { dst, src: 0 });
                self.scratch.reverse();
                pack_pop(
                    &mut self.buf,
                    &mut self.pc_to_span,
                    self.cur_span,
                    &self.scratch,
                );
            }
            ir::Instr::Noop {} => {}
            ir::Instr::Bin {
                op, dst, lhs, rhs, ..
            } => {
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
            ir::Instr::BinImm {
                op, dst, lhs, imm, ..
            } => {
                let dst = self.ensure_register(dst.id);
                let lhs = self.ensure_register(*lhs);
                let imm = *imm;

                macro_rules! emit_bin_imms {
                    ($($ir:ident => $op:ident),* $(,)?) => {
                        match op {
                            $(ir::BinOp::$ir => Op::$op { dst, lhs, imm },)*
                            _ => unreachable!("only integer immediate binops are represented as BinImm"),
                        }
                    };
                }

                self.emit(emit_bin_imms! {
                    IAdd => IAddI,
                    ISub => ISubI,
                    IMul => IMulI,
                    IDiv => IDivI,
                    IEq  => IEqI,
                    IGt  => IGtI,
                    ILt  => ILtI,
                });
            }
        }
    }

    /// Strip [`Op::Nop`]s left behind by [`opt::bc`] and patch every absolute pc
    /// (jump targets, call/tail targets, function entry pcs in
    /// [`Cc::functions`]) through an old->new pc remap. Must run after all
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
                Op::Jmp { target } | Op::JmpT { target, .. } | Op::JmpF { target, .. } => {
                    *target = old_to_new[*target as usize];
                }
                Op::Call { func } | Op::Tail { func } => {
                    *func = old_to_new[*func as usize] as u32;
                }
                _ => {}
            }
            if !matches!(op, Op::Nop) {
                bc[w] = op;
                self.pc_to_span[w] = self.pc_to_span[r];
                w += 1;
            }
        }
        bc.truncate(w);
        self.pc_to_span.truncate(w);

        for f in self.functions.values_mut() {
            if let CcFunc::Bc { pc, .. } = f {
                *pc = old_to_new[*pc] as usize;
            }
        }
    }

    pub fn finalize(self, config: VmConfig) -> (Vm, Vec<BuiltinFn>, DebugInfo) {
        let mut vm = Vm::new(config);
        vm.pc = self
            .functions
            .get(&ir::Id(0))
            .and_then(CcFunc::pc)
            .unwrap_or_default();

        let (string_data, strings) = self.strings.into_arena();
        vm.bytecode = self.buf;
        vm.globals = self.globals.into_vec_fn(Value::from);
        vm.strings = strings;
        vm.string_data = string_data;
        (
            vm,
            self.std_fns.into_vec(),
            DebugInfo::new(self.pc_to_span.into_boxed_slice()),
        )
    }

    /// map pc's to function definitions
    #[must_use]
    pub fn function_table(&self) -> HashMap<usize, String> {
        self.functions
            .values()
            .filter_map(|f| {
                let pc = f.pc()?;
                Some((pc, f.name().to_string()))
            })
            .collect()
    }
}

impl Default for Cc<'_> {
    fn default() -> Self {
        Self::new()
    }
}
