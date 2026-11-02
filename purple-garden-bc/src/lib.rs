#![feature(allocator_ext)]
use std::{
    alloc::{Allocator, Global},
    collections::HashMap,
    hash::RandomState,
    ptr::NonNull,
};

pub mod dis;
mod intern;

use crate::intern::Interner;
use purple_garden_allocators::page::Pages;
use purple_garden_ir::{self as ir, Func, Id, TypeId, constant::Const, ptype};
use purple_garden_runtime::{
    AllocType, BuiltinFn, DebugInfo, REGISTER_COUNT, Value, Vm, VmConfig,
    op::{LookupTable, Op, SwitchKind, SwitchTable},
    string,
};
use purple_garden_shared::config::Config;
use purple_garden_shared::xralloc2::{Class, Reg, Xralloc2};

#[derive(Debug, Clone)]
pub enum CcFunc<'fun> {
    Bc {
        name: &'fun str,
        pc: usize,
    },
    /// `idx` is the syscall slot the JIT page was injected at.
    Native {
        name: &'fun str,
        idx: u16,
        /// Returns in xmm0 instead of rax, see `Op::Native`
        returns_double: bool,
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

#[derive(Clone, Copy, Debug)]
pub enum CcCallTarget {
    Bc { pc: usize },
    Native { idx: u16, returns_double: bool },
}

impl From<&CcFunc<'_>> for CcCallTarget {
    fn from(func: &CcFunc<'_>) -> Self {
        match func {
            CcFunc::Bc { pc, .. } => Self::Bc { pc: *pc },
            CcFunc::Native {
                idx,
                returns_double,
                ..
            } => Self::Native {
                idx: *idx,
                returns_double: *returns_double,
            },
        }
    }
}

/// Per-function working state lives in the scratch allocator `S`, the function tables only needed
/// while compiling in `A`, everything handed to the [`vm::Vm`] by [`Cc::finalize`] in neither.
#[derive(Debug, Clone)]
pub struct Cc<'cc, S: Allocator + Clone = Global, A: Allocator + Clone = Global> {
    pub buf: Vec<Op>,
    pub globals: Interner<Const<'cc>>,
    pub std_fns: Interner<BuiltinFn>,
    pub functions: HashMap<Id, CcFunc<'cc>, RandomState, A>,
    /// Native code and its entry address, retained only for diagnostic dumps and perf maps.
    pub native_code: Option<Vec<(&'cc str, usize, Vec<u8>)>>,
    /// Syscall slot of the native entry function (`Id(0)`), if it compiled to
    /// JIT. When present, `Program::run` can jump directly into the native page
    /// instead of starting from the bytecode trampoline.
    entry_native_idx: Option<u16>,
    /// Every string constant, packed into one allocation that never moves.
    /// Laid out before lowering so native code can embed the addresses;
    /// `str_values[slot]` is the string in `globals` slot `slot`,
    /// [`Value::UNDEF`] for other constants.
    const_pool: Box<[u8]>,
    str_values: Vec<Value>,
    /// `pc_to_span[pc]` is the byte offset into the source of the AST node
    /// that produced the op at `pc`. Threaded into Vm by `Cc::finalize` so
    /// runtime traps can be rendered with <file:line:col>. Parallel to
    /// `self.buf`.
    pub pc_to_span: Vec<u32>,
    /// `labels[label]` is the absolute pc a jump to `label` lands on, after lowering. Labels below
    /// the function's block count are its block ids, the rest come from [`Cc::label`] for code
    /// without a block. `u16::MAX` marks blocks that weren't emitted (e.g.,
    /// tombstoned blocks). Block ids are dense per-function so a Vec
    /// indexed by id beats a `HashMap` on both alloc cost and lookup speed.
    labels: Vec<u16, S>,
    /// Tables of the emitted [`Op::Switch`]es. Targets are block ids until the
    /// end of [`Cc::cc`] maps them to pcs, like jump targets.
    pub switch_tables: Vec<Box<SwitchTable>>,
    /// Tables of the emitted [`Op::Lookup`]s.
    pub lookup_tables: Vec<Box<LookupTable>>,
    /// Entry points of the functions compiled natively, for native callers.
    native_fns: purple_garden_jit::Natives<A>,
    /// purple gardens linear scan operating system and architecture overarching register allocator
    regalloc: Xralloc2<S>,
    /// Set once per IR Instr / Terminator before lowering, consumed by
    /// every `emit` call within that lowering. Saves threading a span
    /// argument through every `self.buf.push(op)` call site.
    cur_span: u32,
    /// Liveness position being lowered, one per row in lockstep with
    /// [`Func::live_set_into`]. Values are allocated at it on their first touch.
    pos: u32,
    /// Liveness and placement hints of the function being compiled, read by
    /// [`Cc::ensure_register`] when a value is first touched.
    live_set: Vec<(u32, u32), S>,
    arg_hints: Vec<Option<u8>, S>,
    /// General-purpose `u8` scratch used by prologue/epilogue and call-site
    /// spills. Callers fill it, then immediately consume it via the
    /// `pack_push`/`pack_pop` free functions; it is never live across a
    /// call to another `Cc` method.
    scratch: Vec<u8, S>,
    /// Reusable `(src, dst)` buffer for the parallel-move resolver in
    /// [`Cc::emit_arg_shuffle`]. Replaces the per-call `todo: Vec<(u8,u8)>`
    /// allocation; taken out of `self` for the duration of each shuffle
    /// and restored on exit so the capacity survives across calls.
    scratch_pairs: Vec<(u8, u8), S>,
    /// Reusable native-code buffer for the JIT, refilled per function. Empty
    /// and untouched when `--no-jit` is set.
    jit: purple_garden_jit::Jit<S>,
}

const _: () = assert!(REGISTER_COUNT <= 64, "xralloc2 tracks registers in a u64");

/// AArch64 inspired register split: r0..r18 caller-saved (args in r0.., result in r0), r19..r31 callee-saved.
const CALLEE_SAVED: u8 = 19;

/// Every vm register, r0 last so xralloc2 hands out the lowest free one first, which also puts
/// the caller-saved registers before the callee-saved ones.
const POOL: [u8; REGISTER_COUNT] = {
    let mut pool = [0; REGISTER_COUNT];
    let mut i = 0;
    while i < REGISTER_COUNT {
        pool[i] = (REGISTER_COUNT - 1 - i) as u8;
        i += 1;
    }
    pool
};

fn take<T, S: Allocator + Clone>(v: &mut Vec<T, S>) -> Vec<T, S> {
    let scratch = v.allocator().clone();
    std::mem::replace(v, Vec::new_in(scratch))
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

impl Cc<'_> {
    #[must_use]
    pub fn new() -> Self {
        Self::new_in(Global, Global)
    }
}

impl<'cc, S: Allocator + Clone, A: Allocator + Clone> Cc<'cc, S, A> {
    #[must_use]
    pub fn new_in(scratch: S, alloc: A) -> Self {
        Self {
            buf: Vec::with_capacity(64),
            pc_to_span: Vec::with_capacity(64),
            globals: Interner::new(),
            std_fns: Interner::new(),
            functions: HashMap::new_in(alloc.clone()),
            native_code: None,
            entry_native_idx: None,
            const_pool: Box::new([]),
            str_values: Vec::new(),
            labels: Vec::new_in(scratch.clone()),
            switch_tables: Vec::new(),
            lookup_tables: Vec::new(),
            native_fns: HashMap::new_in(alloc),
            regalloc: Xralloc2::new_in(scratch.clone()),
            cur_span: 0,
            pos: 0,
            live_set: Vec::new_in(scratch.clone()),
            arg_hints: Vec::new_in(scratch.clone()),
            scratch: Vec::new_in(scratch.clone()),
            scratch_pairs: Vec::new_in(scratch.clone()),
            jit: purple_garden_jit::Jit::new_in(scratch),
        }
    }

    fn intern(&mut self, constant: &'cc Const<'cc>) -> u32 {
        self.globals.intern(constant.clone())
    }

    fn emit(&mut self, op: Op) -> usize {
        let pc = self.buf.len();
        self.buf.push(op);
        self.pc_to_span.push(self.cur_span);
        pc
    }

    /// The register of `id`, allocated on its first touch: its def or, for a block param, the
    /// first edge passing it, which comes before its interval starts.
    fn ensure_register(&mut self, id: Id) -> u8 {
        if let Some(reg) = self.regalloc.reg(id) {
            return reg.num();
        }
        let (start, end) = self.live_set[id.0 as usize];
        assert!(start != u32::MAX, "%v{} has no live interval", id.0);
        let at = self.pos.min(start);
        let reg = match self.arg_hints.get(id.0 as usize).copied().flatten() {
            Some(hint) => self
                .regalloc
                .alloc_with_hint(id, Class::Gpr, at, end, Reg::Gpr(hint)),
            None => self.regalloc.alloc(id, Class::Gpr, at, end),
        };
        let Some(reg) = reg else {
            todo!(
                "no register left for %v{}, there is no stack handling yet",
                id.0
            )
        };
        reg.num()
    }

    /// Compile a list of ir functions to bytecode instructions
    /// Native code goes into pages from `code`.
    pub fn compile<F: Allocator, P: Pages>(
        &mut self,
        config: &Config,
        ir: &'cc [Func<'cc, F>],
        code: P,
    ) -> Result<Option<purple_garden_jit::CodeArena<P>>, String> {
        let mut arena = if config.no_jit {
            None
        } else {
            Some(purple_garden_jit::CodeArena::new_in(code)?)
        };
        self.native_code =
            (config.disassemble > 0 || config.perf_map).then(|| Vec::with_capacity(ir.len()));
        self.functions.reserve(ir.len());

        // Native code embeds string constants by address, so the pool is laid
        // out before any function is compiled, entries padded so each header
        // stays aligned.
        let mut strs: Vec<(u32, &'cc str), S> = Vec::new_in(self.live_set.allocator().clone());
        for func in ir.iter().filter(|f| !f.unused) {
            for block in func.blocks.iter().filter(|b| !b.tombstone) {
                for instr in &block.instructions {
                    if let ir::Instr::LoadConst {
                        value: value @ Const::Str(s),
                        ..
                    } = instr
                        && !self.globals.map.contains_key(value)
                    {
                        strs.push((self.intern(value), s));
                    }
                    if let ir::Instr::Lookup {
                        entries, default, ..
                    } = instr
                    {
                        for value in func
                            .entries(*entries)
                            .iter()
                            .flat_map(|(k, v)| [k, v])
                            .chain([default])
                        {
                            if let Const::Str(s) = value
                                && !self.globals.map.contains_key(value)
                            {
                                strs.push((self.intern(value), s));
                            }
                        }
                    }
                }
                // switch_fold dropped the LoadConsts of a string switch's keys,
                // they only live in its cases now. Lay them out too, back to
                // back, so the switch's jump table has few holes.
                if let Some(ir::Terminator::Switch { cases, .. }) = &block.term {
                    for case in func.cases(*cases) {
                        if let Const::Str(s) = &case.key
                            && !self.globals.map.contains_key(&case.key)
                        {
                            strs.push((self.intern(&case.key), s));
                        }
                    }
                }
            }
        }
        let size = strs.iter().map(|(_, s)| string::padded_size(s.len())).sum();
        // len == capacity, so `into_boxed_slice` keeps the allocation and the
        // pointers written below stay valid.
        let mut pool = vec![0u8; size];
        let base = pool.as_mut_ptr();
        debug_assert!(
            size == 0 || (base as usize) % string::ALIGN == 0,
            "const pool base must be aligned (got {base:p})",
        );
        let mut offset = 0;
        for (slot, s) in strs {
            let hash = string::hash(s.as_bytes());
            let value = unsafe { string::write_payload(base.add(offset), s, hash) };
            offset += string::padded_size(s.len());
            let slot = slot as usize;
            if self.str_values.len() <= slot {
                self.str_values.resize(slot + 1, Value::UNDEF);
            }
            self.str_values[slot] = value;
        }
        self.const_pool = pool.into_boxed_slice();
        debug_assert_eq!(self.const_pool.as_ptr(), base);

        for func in ir.iter().filter(|f| !f.unused) {
            self.cc(func, arena.as_mut())?;
        }

        if let Some(arena) = &mut arena {
            arena.seal()?;
        }
        Ok(arena)
    }

    fn cc<F: Allocator>(
        &mut self,
        fun: &'cc Func<'cc, F>,
        native: Option<&mut purple_garden_jit::CodeArena<impl Pages>>,
    ) -> Result<(), String> {
        // Taken out so try_compile_native can borrow it next to `&mut self`
        let mut live_set = take(&mut self.live_set);
        fun.live_set_into(&mut live_set);

        // Native functions are injected as syscalls. The entry function can
        // be run directly from the native page when it compiles to JIT.
        let is_root = fun.id == ir::Id(0);
        if let Some(native) = native
            && self.try_compile_native(fun, &live_set, native)
        {
            purple_garden_shared::trace!("[bc::Cc::cc][{}] native", fun.name);
            if is_root {
                let Some(&CcFunc::Native { idx, .. }) = self.functions.get(&fun.id) else {
                    unreachable!("try_compile_native inserts a Native CcFunc on success");
                };
                self.entry_native_idx = Some(idx);
            }
            self.live_set = live_set;
            return Ok(());
        }
        self.live_set = live_set;

        fun.arg_hints_into(&mut self.arg_hints);
        self.regalloc.reset(self.live_set.len(), &POOL, &[]);
        if let Some(entry) = fun.blocks.first()
            && !entry.tombstone
        {
            for (i, &param) in fun.params(entry.params).iter().enumerate() {
                let (start, end) = self.live_set[param.0 as usize];
                if start != u32::MAX {
                    self.regalloc.try_alloc_at(param, Reg::Gpr(i as u8), end);
                }
            }
        }

        // binding the id of a function to its context
        let pc = self.buf.len();
        let tables = self.switch_tables.len();
        self.functions
            .insert(fun.id, CcFunc::Bc { pc, name: fun.name });

        self.cur_span = fun.span;

        // labels is indexed by ir block id, and block ids restart at 0 per function; size it to
        // the current function's block count and fill with the u16::MAX sentinel for
        // tombstoned/unemitted blocks.
        self.labels.clear();
        self.labels.resize(fun.blocks.len(), u16::MAX);

        self.pos = 0;
        for (idx, block) in fun.blocks.iter().enumerate() {
            if block.tombstone {
                continue;
            }

            self.bind(block.id.0 as u16);

            // Params no edge touched yet start here
            for &param in fun.params(block.params) {
                if self.live_set[param.0 as usize].0 != u32::MAX {
                    self.ensure_register(param);
                }
            }
            self.pos += 1; // block params row
            for instruction in &block.instructions {
                self.cur_span = instruction.span();
                self.instr(fun, instruction);
                self.pos += 1;
            }

            if let Some(term) = block.term.as_ref() {
                self.cur_span = term.span();
            }
            // The next emitted block, an edge to it falls through instead of jumping
            let next_block = fun.blocks[idx + 1..]
                .iter()
                .find(|b| !b.tombstone)
                .map(|b| b.id);
            self.term(fun, block.term.as_ref(), next_block);
            self.pos += 1; // terminator row
        }

        // The entry/root function is entered directly by the VM, not through a
        // Call, so its last block falls off the end of the buffer instead of
        // returning. Emit a trailing Ret so a syscall trap raised at top level
        // is drained: pending_trap is only checked on Op::Ret. The synthetic
        // root frame in Vm::new gives this Ret a frame to pop.
        if is_root
            && self.entry_native_idx.is_none()
            && !matches!(self.buf.last(), Some(Op::Ret | Op::Tail { .. }))
        {
            self.emit(Op::Ret);
        }

        // The root has no caller to restore registers for
        let max_reg = (CALLEE_SAVED..REGISTER_COUNT as u8)
            .rev()
            .find(|&r| self.regalloc.used(Reg::Gpr(r)));
        if !is_root && let Some(max_reg) = max_reg {
            self.cur_span = fun.span;
            self.emit_frame(pc, CALLEE_SAVED, max_reg);
        }

        for i in pc..self.buf.len() {
            self.buf[i] = match self.buf[i] {
                Op::JmpT { cond, target } => Op::JmpT {
                    cond,
                    target: self.labels[target as usize],
                },
                Op::JmpF { cond, target } => Op::JmpF {
                    cond,
                    target: self.labels[target as usize],
                },
                Op::JmpEqI { lhs, imm, target } => Op::JmpEqI {
                    lhs,
                    imm,
                    target: self.labels[target as usize],
                },
                Op::JmpNeI { lhs, imm, target } => Op::JmpNeI {
                    lhs,
                    imm,
                    target: self.labels[target as usize],
                },
                Op::JmpSEq { lhs, rhs, target } => Op::JmpSEq {
                    lhs,
                    rhs,
                    target: self.labels[target as usize],
                },
                Op::JmpSNe { lhs, rhs, target } => Op::JmpSNe {
                    lhs,
                    rhs,
                    target: self.labels[target as usize],
                },
                Op::Jmp { target } => Op::Jmp {
                    target: self.labels[target as usize],
                },
                other => other,
            };
        }
        for table in &mut self.switch_tables[tables..] {
            for target in &mut table.targets {
                *target = self.labels[*target as usize];
            }
        }

        purple_garden_shared::trace!("[bc::Cc::cc][{}] size={}", fun.name, self.buf.len() - pc);

        Ok(())
    }

    fn try_compile_native<F: Allocator>(
        &mut self,
        fun: &Func<'cc, F>,
        liveness: &[(u32, u32)],
        arena: &mut purple_garden_jit::CodeArena<impl Pages>,
    ) -> bool {
        let Some(()) = self.jit.compile_func_with_liveness(
            fun,
            liveness,
            &self.globals.map,
            &self.str_values,
            &self.native_fns,
            arena.next_entry(),
        ) else {
            purple_garden_shared::trace!("[bc::Cc::cc] native skipped function {}", fun.name);
            return false;
        };
        let Some(entry) = arena.push(self.jit.code()) else {
            purple_garden_shared::trace!(
                "[bc::Cc::cc] code arena full, {} stays bytecode",
                fun.name
            );
            return false;
        };

        // Retain a copy only when diagnostics need it.
        if let Some(native_code) = &mut self.native_code {
            native_code.push((fun.name, entry as usize, self.jit.code().to_vec()));
        }
        self.native_fns.insert(fun.id, entry);
        let idx = self.std_fns.intern(entry) as u16;
        self.functions.insert(
            fun.id,
            CcFunc::Native {
                name: fun.name,
                idx,
                returns_double: fun.ret == Some(ptype::Type::Double),
            },
        );
        true
    }

    /// Move `args[i]` into the i-th argument register (`r0..r{N-1}`) for a call
    /// or tail. `r0` is both the first argument and the return-value slot.
    ///
    /// Parallel-move: emit a direct Mov for any pending pair whose
    /// dst isn't another pending move's src. When all that remains is one
    /// or more cycles (e.g. swap r0,r1), break them through a free register,
    /// or fall back to push and pop for those leftovers only.
    fn emit_arg_shuffle(&mut self, args: &[Id]) {
        let mut todo = take(&mut self.scratch_pairs);
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

            // All remaining moves form cycles. Borrow a free register above the arg zone that
            // isn't a src or dst in any pending move, and use it to break one cycle at a time
            // without touching the spill stack
            let arg_end = args.len() as u8;
            let scratch = (arg_end..REGISTER_COUNT as u8)
                .filter(|&r| !todo.iter().any(|(s, d)| *s == r || *d == r))
                .find_map(|r| self.regalloc.try_take(Reg::Gpr(r)))
                .map(Reg::num);

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
                while let Some(idx) = todo.iter().position(|(_, d)| *d == cur_freed) {
                    let (src, dst) = todo.swap_remove(idx);
                    self.emit(Op::Mov { dst, src });
                    cur_freed = src;
                }
                self.emit(Op::Mov {
                    dst: start_dst,
                    src: scratch,
                });
                self.regalloc.give_back(Reg::Gpr(scratch));
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

    /// Push r{lo}..r{max_reg} onto the spill stack (callee-saved prologue), `lo` is
    /// [`CALLEE_SAVED`] or past `max_reg` for the root.
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

    /// A new jump target without a block, unbound until [`Cc::bind`].
    fn label(&mut self) -> u16 {
        let label = self.labels.len() as u16;
        self.labels.push(u16::MAX);
        label
    }

    /// Land jumps to `label` on the next emitted op.
    fn bind(&mut self, label: u16) {
        self.labels[label as usize] = self.buf.len() as u16;
    }

    /// The body of the function at `pc` was emitted as if it saved no registers. Lay out its frame
    /// like the JIT does: returns jump to one shared epilogue, tail calls to a stub restoring the
    /// registers before the tail call, and the prologue is rotated in front of the body. The new
    /// jump targets get their own labels.
    fn emit_frame(&mut self, pc: usize, lo: u8, max_reg: u8) {
        let body_end = self.buf.len();
        let epilogue = self.label();
        self.bind(epilogue);
        self.emit_epilogue(lo, max_reg);
        self.emit(Op::Ret);

        for i in pc..body_end {
            match self.buf[i] {
                // Falls through into the epilogue
                Op::Ret if i + 1 == body_end => self.buf[i] = Op::Nop,
                Op::Ret => self.buf[i] = Op::Jmp { target: epilogue },
                Op::Tail { func } => {
                    let stub = self.label();
                    self.bind(stub);
                    self.emit_epilogue(lo, max_reg);
                    self.emit(Op::Tail { func });
                    self.buf[i] = Op::Jmp { target: stub };
                }
                _ => {}
            }
        }

        let end = self.buf.len();
        self.emit_prologue(lo, max_reg);
        let len = self.buf.len() - end;
        self.buf[pc..].rotate_right(len);
        self.pc_to_span[pc..].rotate_right(len);
        for at in &mut self.labels {
            if *at != u16::MAX {
                *at += len as u16;
            }
        }
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

    /// Lower a conditional branch, `jump_no(target)` leaves for `target` if the condition fails.
    /// The jump reads the condition before any edge move runs, so a move may reuse its register.
    /// Each edge then moves on its own path: a no edge with moves gets its own label
    /// behind the yes path.
    fn branch<F: Allocator>(
        &mut self,
        fun: &Func<'cc, F>,
        yes: (ir::Id, ir::ParamsId),
        no: (ir::Id, ir::ParamsId),
        next_block: Option<ir::Id>,
        jump_no: impl FnOnce(u16) -> Op,
    ) {
        if !self.has_moves(fun, no) {
            self.emit(jump_no(no.0.0 as u16));
            self.edge(fun, yes, next_block);
            return;
        }

        let no_path = self.label();
        self.emit(jump_no(no_path));
        self.edge(fun, yes, None);
        self.bind(no_path);
        self.edge(fun, no, next_block);
    }

    fn has_moves<F: Allocator>(
        &mut self,
        fun: &Func<'cc, F>,
        (target, params): (ir::Id, ir::ParamsId),
    ) -> bool {
        let dst = fun.params(fun.blocks[target.0 as usize].params);
        fun.params(params)
            .iter()
            .zip(dst)
            .any(|(&src, &dst)| self.ensure_register(src) != self.ensure_register(dst))
    }

    /// Move `params` into the params of `target`, then jump there unless it is `next_block`.
    fn edge<F: Allocator>(
        &mut self,
        fun: &Func<'cc, F>,
        (target, params): (ir::Id, ir::ParamsId),
        next_block: Option<ir::Id>,
    ) {
        let dst = fun.params(fun.blocks[target.0 as usize].params);
        for (&src, &dst) in fun.params(params).iter().zip(dst) {
            let src = self.ensure_register(src);
            let dst = self.ensure_register(dst);
            if src != dst {
                self.emit(Op::Mov { dst, src });
            }
        }
        if Some(target) != next_block {
            self.emit(Op::Jmp {
                target: target.0 as u16,
            });
        }
    }

    fn term<F: Allocator>(
        &mut self,
        fun: &Func<'cc, F>,
        t: Option<&ir::Terminator>,
        next_block: Option<ir::Id>,
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
                self.emit(Op::Ret);
            }
            ir::Terminator::Jump { id, params, .. } => {
                self.edge(fun, (*id, *params), next_block);
            }
            ir::Terminator::Branch { cond, yes, no, .. } => {
                let cond = self.ensure_register(*cond);
                self.branch(fun, *yes, *no, next_block, |target| Op::JmpF {
                    cond,
                    target,
                });
            }
            ir::Terminator::BranchCmpImm {
                op,
                lhs,
                imm,
                yes,
                no,
                ..
            } => {
                let ir::BinOp::IEq = op else {
                    unreachable!("branch_cmp only emits supported comparison ops");
                };
                let lhs = self.ensure_register(*lhs);
                let imm = *imm;
                self.branch(fun, *yes, *no, next_block, |target| Op::JmpNeI {
                    lhs,
                    imm,
                    target,
                });
            }
            ir::Terminator::BranchCmp {
                op,
                lhs,
                rhs,
                yes,
                no,
                ..
            } => {
                let ir::BinOp::SEq = op else {
                    unreachable!("branch_cmp only emits supported comparison ops");
                };
                let lhs = self.ensure_register(*lhs);
                let rhs = self.ensure_register(*rhs);
                self.branch(fun, *yes, *no, next_block, |target| Op::JmpSNe {
                    lhs,
                    rhs,
                    target,
                });
            }
            ir::Terminator::Switch {
                subject,
                cases,
                default,
                ..
            } => {
                let subject = self.ensure_register(*subject);
                let cases = fun.cases(*cases);
                let default = default.0.0 as u16;
                let block = |case: &ir::Case| case.target.0.0 as u16;

                // Strings are interned, so a string key is its address in the
                // pool, where strings sit ALIGN bytes apart.
                let (kind, step) = match cases[0].key {
                    Const::Str(_) => (SwitchKind::Str, string::ALIGN as i64),
                    _ => (SwitchKind::Int, 1),
                };
                let key = |case: &ir::Case| match &case.key {
                    Const::Str(_) => self.str_values[self.globals.map[&case.key] as usize].0 as i64,
                    Const::Int(v) => *v,
                    _ => unreachable!("switch_fold only folds Str and Int keys"),
                };
                let first = cases.iter().map(key).min().unwrap();
                let slot = |case: &ir::Case| ((key(case) - first) / step) as usize;
                let len = cases.iter().map(slot).max().unwrap() + 1;

                let mut targets = vec![default; len].into_boxed_slice();
                for case in cases {
                    targets[slot(case)] = block(case);
                }
                let table = Box::new(SwitchTable {
                    first: first as u64,
                    targets,
                });
                let ptr = NonNull::from(&*table);
                self.switch_tables.push(table);
                self.emit(Op::Switch {
                    kind,
                    subject,
                    table: ptr,
                });

                if Some(ir::Id(u32::from(default))) != next_block {
                    self.emit(Op::Jmp { target: default });
                }
            }
            ir::Terminator::Tail { func, args, .. } => {
                let Some(target) = self.functions.get(func).map(CcCallTarget::from) else {
                    unreachable!();
                };

                let args = fun.params(*args);
                // If the tail target needs a wider argument zone than this the shuffle would write
                // into registers that this function still owes back to its caller. Fall back to a
                // normal call so the epilogue can restore them after the callee returns.
                let tail_clobbers_callee_saved = args.len() as u8 > CALLEE_SAVED;
                self.emit_arg_shuffle(args);

                if tail_clobbers_callee_saved {
                    match target {
                        CcCallTarget::Bc { pc } => self.emit(Op::Call { func: pc as u32 }),
                        CcCallTarget::Native {
                            idx,
                            returns_double,
                        } => self.emit(Op::Native {
                            idx,
                            returns_double,
                        }),
                    };
                    self.emit(Op::Ret);
                } else {
                    match target {
                        CcCallTarget::Bc { pc } => {
                            self.emit(Op::Tail { func: pc as u32 });
                        }
                        CcCallTarget::Native {
                            idx,
                            returns_double,
                        } => {
                            self.emit(Op::Native {
                                idx,
                                returns_double,
                            });
                            self.emit(Op::Ret);
                        }
                    }
                };
            }
        }
    }

    /// `owner` is the function `i` belongs to, it holds the lists `i` refers to.
    fn instr<F: Allocator>(&mut self, owner: &'cc Func<'cc, F>, i: &'cc ir::Instr<'cc>) {
        let pos = self.pos;
        match i {
            ir::Instr::Store {
                src, base, offset, ..
            } => {
                let src = self.ensure_register(*src);
                let base = self.ensure_register(*base);
                self.emit(Op::Store {
                    base,
                    offset: *offset,
                    src,
                });
            }
            ir::Instr::Load {
                dst, base, offset, ..
            } => {
                let dst = self.ensure_register(dst.id);
                let base = self.ensure_register(*base);
                self.emit(Op::Load {
                    dst,
                    base,
                    offset: *offset,
                });
            }
            ir::Instr::AddrOf {
                dst, base, offset, ..
            } => {
                let dst = self.ensure_register(dst.id);
                let base = self.ensure_register(*base);
                self.emit(Op::AddrOf {
                    dst,
                    base,
                    offset: *offset,
                });
            }
            ir::Instr::Alloc {
                dst: TypeId { id, ty },
                layout,
                ..
            } => {
                let dst = self.ensure_register(*id);
                let Some(kind) = AllocType::from_ty(ty) else {
                    unreachable!("alloc attempts to alloc non heap allocated value?");
                };
                let (size, align) = (layout.size() as u32, layout.align() as u8);
                self.emit(Op::Alloc {
                    dst,
                    kind,
                    size,
                    align,
                });
            }
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
            ir::Instr::Lookup {
                dst,
                subject,
                entries,
                default,
                ..
            } => {
                let entries = owner.entries(*entries);
                // Slots as for Op::Switch, holding values instead of pcs.
                let (kind, step) = match entries[0].0 {
                    Const::Str(_) => (SwitchKind::Str, string::ALIGN as i64),
                    _ => (SwitchKind::Int, 1),
                };
                let value = |c: &Const<'cc>| match c {
                    Const::Str(_) => self.str_values[self.globals.map[c] as usize],
                    c => Value::from(c.clone()),
                };
                let key = |c: &Const<'cc>| value(c).0 as i64;
                let first = entries.iter().map(|(k, _)| key(k)).min().unwrap();
                let slot = |k: &Const<'cc>| ((key(k) - first) / step) as usize;
                let len = entries.iter().map(|(k, _)| slot(k)).max().unwrap() + 1;
                let mut values = vec![value(default); len].into_boxed_slice();
                for (k, v) in entries {
                    values[slot(k)] = value(v);
                }
                let table = Box::new(LookupTable {
                    first: first as u64,
                    values,
                    default: value(default),
                });
                let dst = self.ensure_register(dst.id);
                let subject = self.ensure_register(*subject);
                let ptr = NonNull::from(&*table);
                self.lookup_tables.push(table);
                self.emit(Op::Lookup {
                    kind,
                    dst,
                    subject,
                    table: ptr,
                });
            }
            ir::Instr::LoadConst { dst, value, .. } => {
                let dst = self.ensure_register(dst.id);
                match value {
                    Const::Int(i) if *i > i32::MIN as i64 && *i < i32::MAX as i64 => {
                        self.emit(Op::LoadI {
                            dst,
                            value: *i as i32,
                        });
                    }
                    Const::True | Const::False => {
                        // fast path for booleans
                        let value = matches!(value, Const::True) as i32;
                        self.emit(Op::LoadI { dst, value });
                    }
                    // a Void value is never read, whatever the register holds will do
                    Const::Undefined => {}
                    _ => {
                        let idx = self.intern(value);
                        self.emit(Op::LoadG { dst, idx });
                    }
                }
            }
            ir::Instr::Call {
                dst, func, args, ..
            } => {
                let args = owner.params(*args);
                let Some(target) = self.functions.get(func).map(CcCallTarget::from) else {
                    unreachable!();
                };

                // The callee may clobber every caller-saved register and its arg zone, it
                // preserves the rest. The caller spills the live values in r0..r{clobber_end-1}.
                let clobber_end = CALLEE_SAVED.max(args.len() as u8);
                self.scratch.clear();
                for reg in self.regalloc.live_after(pos) {
                    if reg.num() < clobber_end {
                        purple_garden_shared::trace!(
                            "[bc] spilled r{} at call_idx={}",
                            reg.num(),
                            pos
                        );
                        self.scratch.push(reg.num());
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
                match target {
                    CcCallTarget::Bc { pc } => self.emit(Op::Call { func: pc as u32 }),
                    CcCallTarget::Native {
                        idx,
                        returns_double,
                    } => self.emit(Op::Native {
                        idx,
                        returns_double,
                    }),
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
            ir::Instr::Sys { dst, fun, args, .. } => {
                let args = owner.params(*args);
                let idx = self.std_fns.intern(fun.ptr);

                // Syscall convention: shuffle writes r0..r{argcount-1}, syscall
                // body writes r0 (return slot). Clobber range is r0..r{clobber_end-1}
                // (clobber_end = args.len().max(1)). Builtins never touch higher
                // registers, so only spill alive-across values inside that range.
                let clobber_end = args.len().max(1) as u8;
                self.scratch.clear();
                for reg in self.regalloc.live_after(pos) {
                    if reg.num() < clobber_end {
                        self.scratch.push(reg.num());
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
                self.emit(Op::Sys {
                    idx: idx as u16,
                    argc: args.len() as u8,
                });
                self.emit(Op::Mov { dst, src: 0 });
                self.scratch.reverse();
                pack_pop(
                    &mut self.buf,
                    &mut self.pc_to_span,
                    self.cur_span,
                    &self.scratch,
                );
            }
            ir::Instr::Assert { cond, .. } => {
                let cond = self.ensure_register(*cond);
                self.emit(Op::Assert { cond });
                // a Void value is never read, not reserving a value for assert is fine :)
                // self.ensure_register(dst.id);
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
                    IAdd, ISub, IMul, IDiv, IMod, IAnd, ILt, IGt, IEq,
                    DAdd, DSub, DMul, DDiv, DLt, DGt,
                    BEq, SEq
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
                    IMod => IModI,
                    IAnd => IAndI,
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

        // PERF: do classification of possible targets with simd

        // Skip the whole pass when peephole produced nothing to compact.
        // Avoids the remap allocation and second walk in the common no-op case.
        if !bc.iter().any(|op| matches!(op, Op::Nop)) {
            return;
        }

        // bc.len() fits in u16 since Jmp.target is u16; halve the remap
        // table's cache footprint vs Vec<u32>.
        let mut old_to_new = Vec::with_capacity_in(bc.len() + 1, self.live_set.allocator().clone());
        old_to_new.resize(bc.len() + 1, 0u16);
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
                Op::Jmp { target }
                | Op::JmpT { target, .. }
                | Op::JmpF { target, .. }
                | Op::JmpEqI { target, .. }
                | Op::JmpNeI { target, .. }
                | Op::JmpSEq { target, .. }
                | Op::JmpSNe { target, .. } => {
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

        for table in &mut self.switch_tables {
            for target in &mut table.targets {
                *target = old_to_new[*target as usize];
            }
        }

        for f in self.functions.values_mut() {
            if let CcFunc::Bc { pc, .. } = f {
                *pc = old_to_new[*pc] as usize;
            }
        }
    }

    /// Returns the vm, a list of syscalls, debug info, and the entry point to the native page, if
    /// jitted
    /// The VM's GC takes its pages from `alloc`.
    pub fn finalize<'vm>(
        self,
        config: VmConfig,
        alloc: impl Allocator + 'vm,
    ) -> (Vm<'vm>, Vec<BuiltinFn>, DebugInfo, Option<u16>) {
        let Cc {
            mut buf,
            globals,
            std_fns,
            functions,
            entry_native_idx,
            mut pc_to_span,
            const_pool,
            str_values,
            switch_tables,
            lookup_tables,
            ..
        } = self;

        // entry is lowered last, we halt after its Ret. Enables omitting bounds check in dispatch loop
        buf.push(Op::Halt);
        pc_to_span.push(0);

        let mut vm = Vm::new_in(config, alloc);
        // A native entry runs directly from its native page; a bytecode entry
        // from its own first op.
        vm.pc = entry_native_idx
            .map(|_| 0)
            .or_else(|| functions.get(&ir::Id(0)).and_then(CcFunc::pc))
            .unwrap_or_default();

        vm.bytecode = buf;
        vm.switch_tables = switch_tables;
        vm.lookup_tables = lookup_tables;

        debug_assert_eq!(
            globals
                .map
                .keys()
                .filter(|c| matches!(c, Const::Str(_)))
                .count(),
            str_values.iter().filter(|&&v| v != Value::UNDEF).count(),
            "every string constant is laid out before lowering",
        );
        vm.globals = globals.into_vec_map(|constant| match constant {
            // Filled from str_values below, the closure doesn't see the slot.
            Const::Str(_) => Value::UNDEF,
            constant => Value::from(constant),
        });
        for (slot, &value) in str_values.iter().enumerate() {
            if value != Value::UNDEF {
                vm.globals[slot] = value;
                vm.strings.insert(value);
            }
        }
        vm.const_pool = const_pool;

        (
            vm,
            std_fns.into_vec(),
            DebugInfo::new(pc_to_span.into_boxed_slice()),
            entry_native_idx,
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

#[cfg(test)]
mod tests {
    use super::*;
    use ir::BinOp;
    use ir::{Block, EMPTY_PARAMS, Instr, Terminator, ptype::Type};
    use purple_garden_allocators::page::PageAlloc;
    use std::alloc::Layout;

    #[test]
    fn compact_nops_remaps_switch_targets() {
        let mut cc = Cc::new();
        cc.buf = vec![Op::Nop, Op::Halt, Op::Nop, Op::Ret];
        cc.pc_to_span = vec![0; cc.buf.len()];
        cc.switch_tables.push(Box::new(SwitchTable {
            first: 0,
            targets: Box::new([1, 3]),
        }));
        cc.compact_nops();
        assert_eq!(cc.buf, [Op::Halt, Op::Ret]);
        assert_eq!(&*cc.switch_tables[0].targets, [0, 1]);
    }

    fn type_id(id: u32, ty: Type<'static>) -> TypeId<'static> {
        TypeId { id: Id(id), ty }
    }

    fn entry_fun(
        instructions: Vec<Instr<'static>>,
        ret: Option<(Id, Type<'static>)>,
    ) -> Func<'static> {
        let ret_ty = ret.as_ref().map(|(_, ty)| *ty);
        let ret_id = ret.map(|(id, _)| id);
        let mut fun = Func::new("entry", Id(0), Vec::new(), ret_ty);
        fun.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params: EMPTY_PARAMS,
            instructions,
            term: Some(Terminator::Return {
                value: ret_id,
                span: 0,
            }),
        });
        fun
    }

    fn compile_one(fun: Func<'static>) -> Vec<Op> {
        let mut cc = Cc::new();
        let mut config = Config::default();
        config.no_jit = true;
        let funcs = [fun];
        cc.compile(&config, &funcs, PageAlloc {}).unwrap();
        cc.buf.clone()
    }

    fn has_op(ops: &[Op], pred: impl Fn(&Op) -> bool) -> bool {
        ops.iter().any(pred)
    }

    #[test]
    fn lowers_int_load_const_to_load_i() {
        let ops = compile_one(entry_fun(
            vec![Instr::LoadConst {
                dst: type_id(0, Type::Int),
                value: Const::Int(42),
                span: 0,
            }],
            Some((Id(0), Type::Int)),
        ));

        assert!(
            has_op(&ops, |op| matches!(op, Op::LoadI { dst: 0, value: 42 })),
            "expected int constant to lower to LoadI: {:?}",
            ops
        );
    }

    #[test]
    fn lowers_boolean_load_consts_to_immediates() {
        for (constant, expected) in [(Const::True, 1), (Const::False, 0)] {
            let ops = compile_one(entry_fun(
                vec![Instr::LoadConst {
                    dst: type_id(0, Type::Bool),
                    value: constant,
                    span: 0,
                }],
                Some((Id(0), Type::Bool)),
            ));

            assert!(
                has_op(
                    &ops,
                    |op| matches!(op, Op::LoadI { dst: 0, value } if *value == expected)
                ),
                "expected boolean constant to lower to LoadI({expected}): {ops:?}",
            );
            assert!(
                !has_op(&ops, |op| matches!(op, Op::LoadG { .. })),
                "boolean constant should not lower to LoadG: {ops:?}",
            );
        }
    }

    #[test]
    fn lowers_non_immediate_load_const_to_load_global() {
        let ops = compile_one(entry_fun(
            vec![Instr::LoadConst {
                dst: type_id(0, Type::Str),
                value: Const::Str("hello".into()),
                span: 0,
            }],
            Some((Id(0), Type::Str)),
        ));

        assert!(
            has_op(&ops, |op| matches!(op, Op::LoadG { dst: 0, idx: 0 })),
            "expected string constant to lower to LoadG: {:?}",
            ops
        );
    }

    #[test]
    fn lowers_bin_imm_to_matching_bytecode_op() {
        let ops = compile_one(entry_fun(
            vec![
                Instr::LoadConst {
                    dst: type_id(0, Type::Int),
                    value: Const::Int(7),
                    span: 0,
                },
                Instr::BinImm {
                    op: BinOp::IAdd,
                    dst: type_id(1, Type::Int),
                    lhs: Id(0),
                    imm: 35,
                    span: 0,
                },
            ],
            Some((Id(1), Type::Int)),
        ));

        assert!(
            has_op(&ops, |op| matches!(
                op,
                Op::IAddI {
                    dst: 0,
                    lhs: 0,
                    imm: 35
                } | Op::IAddI {
                    dst: 1,
                    lhs: 0,
                    imm: 35
                }
            )),
            "expected BinImm IAdd to lower to IAddI: {:?}",
            ops
        );
    }

    #[test]
    fn lowers_cast_to_bytecode_cast() {
        let ops = compile_one(entry_fun(
            vec![
                Instr::LoadConst {
                    dst: type_id(0, Type::Int),
                    value: Const::Int(42),
                    span: 0,
                },
                Instr::Cast {
                    dst: type_id(1, Type::Double),
                    from: type_id(0, Type::Int),
                    span: 0,
                },
            ],
            Some((Id(1), Type::Double)),
        ));

        assert!(
            has_op(&ops, |op| matches!(
                op,
                Op::CastToDouble { dst: 0, src: 0 } | Op::CastToDouble { dst: 1, src: 0 }
            )),
            "expected int-to-double cast to lower to CastToDouble: {:?}",
            ops
        );
    }

    #[test]
    fn lowers_assert_to_op_assert() {
        let ops = compile_one(entry_fun(
            vec![
                Instr::LoadConst {
                    dst: type_id(0, Type::Bool),
                    value: Const::True,
                    span: 0,
                },
                Instr::Assert {
                    dst: type_id(1, Type::Void),
                    cond: Id(0),
                    span: 0,
                },
            ],
            None,
        ));

        assert!(
            has_op(&ops, |op| matches!(op, Op::Assert { cond: 0 })),
            "expected Instr::Assert to lower to Op::Assert: {ops:?}"
        );
    }

    #[test]
    fn lowers_alloc_and_store() {
        let ops = compile_one(entry_fun(
            vec![
                Instr::Alloc {
                    dst: type_id(0, Type::Record(&[])),
                    layout: Layout::from_size_align(16, 8).unwrap(),
                    span: 0,
                },
                Instr::LoadConst {
                    dst: type_id(1, Type::Int),
                    value: Const::Int(42),
                    span: 0,
                },
                Instr::Store {
                    src: Id(1),
                    base: Id(0),
                    offset: 8,
                    span: 0,
                },
            ],
            None,
        ));

        assert!(
            has_op(&ops, |op| matches!(
                op,
                Op::Alloc {
                    dst: 0,
                    kind: AllocType::Record,
                    size: 16,
                    align: 8
                }
            )),
            "expected Alloc to lower to bytecode Alloc: {:?}",
            ops
        );
        assert!(
            has_op(&ops, |op| matches!(
                op,
                Op::Store {
                    base: 0,
                    offset: 8,
                    src: 1
                }
            )),
            "expected Store to lower to bytecode Store: {:?}",
            ops
        );
    }

    #[test]
    fn lowers_load_to_bytecode_load() {
        let ops = compile_one(entry_fun(
            vec![
                Instr::Alloc {
                    dst: type_id(0, Type::Record(&[])),
                    layout: Layout::from_size_align(16, 8).unwrap(),
                    span: 0,
                },
                Instr::LoadConst {
                    dst: type_id(1, Type::Int),
                    value: Const::Int(42),
                    span: 0,
                },
                Instr::Store {
                    src: Id(1),
                    base: Id(0),
                    offset: 8,
                    span: 0,
                },
                Instr::Load {
                    dst: type_id(2, Type::Int),
                    base: Id(0),
                    offset: 8,
                    span: 0,
                },
            ],
            Some((Id(2), Type::Int)),
        ));

        assert!(
            has_op(&ops, |op| matches!(
                op,
                Op::Load {
                    dst: 0,
                    base: 0,
                    offset: 8
                }
            )),
            "expected Load to lower to bytecode Load: {:?}",
            ops
        );
    }

    #[test]
    fn lowers_addrof_to_bytecode_addrof() {
        let ops = compile_one(entry_fun(
            vec![
                Instr::Alloc {
                    dst: type_id(0, Type::Record(&[])),
                    layout: Layout::from_size_align(16, 8).unwrap(),
                    span: 0,
                },
                Instr::AddrOf {
                    dst: type_id(1, Type::Foreign("ptr")),
                    base: Id(0),
                    offset: 8,
                    span: 0,
                },
            ],
            Some((Id(1), Type::Foreign("ptr"))),
        ));

        assert!(
            has_op(&ops, |op| matches!(
                op,
                Op::AddrOf {
                    dst: 0,
                    base: 0,
                    offset: 8
                }
            )),
            "expected AddrOf to lower to bytecode AddrOf: {:?}",
            ops
        );
    }

    #[test]
    fn lowers_branch_cmp_imm_to_bytecode_branch() {
        let mut fun = Func::new("entry", Id(0), Vec::new(), Some(Type::Int));
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params: EMPTY_PARAMS,
                instructions: vec![Instr::LoadConst {
                    dst: type_id(0, Type::Int),
                    value: Const::Int(1),
                    span: 0,
                }],
                term: Some(Terminator::BranchCmpImm {
                    op: BinOp::IEq,
                    lhs: Id(0),
                    imm: 1,
                    yes: (Id(1), EMPTY_PARAMS),
                    no: (Id(2), EMPTY_PARAMS),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params: EMPTY_PARAMS,
                instructions: vec![Instr::LoadConst {
                    dst: type_id(1, Type::Int),
                    value: Const::Int(10),
                    span: 0,
                }],
                term: Some(Terminator::Return {
                    value: Some(Id(1)),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(2),
                params: EMPTY_PARAMS,
                instructions: vec![Instr::LoadConst {
                    dst: type_id(2, Type::Int),
                    value: Const::Int(20),
                    span: 0,
                }],
                term: Some(Terminator::Return {
                    value: Some(Id(2)),
                    span: 0,
                }),
            },
        ];

        let ops = compile_one(fun);

        assert!(
            has_op(&ops, |op| matches!(
                op,
                Op::JmpEqI { lhs: 0, imm: 1, .. } | Op::JmpNeI { lhs: 0, imm: 1, .. }
            )),
            "expected BranchCmpImm IEq to lower to an immediate compare jump: {:?}",
            ops
        );
    }
}
