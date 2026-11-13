//! x86-64 JIT backend
//!
//! IR is lowered to native code after a pre-pass uses xralloc2 to map SSA values to x86 registers.
//!
//! The native ABI keeps `*mut Vm` in `rbx`. `Vm::r` being the first field, `rbx` is the base of the VM
//! register file. Arguments arrive in `vm.r[0..n]`; return register is `vm.r[0]`. All
//! computation happen in GPRs. The jit moves arguments from `vm.r[0..n]` to GPRs in a jitted
//! functions prologue.
//!
//! A trap records its anomaly in a runtime helper, which leaves every native frame through
//! `jit_abort` back to the interpreter, so calls need no trap check. Native functions
//! preserve `rbx` and clobber only the SysV caller-saved registers, so native
//! to native calls are plain `call`s: no trampoline, no spill of the VM base.
//! The interpreter enters natives through `enter_native` and ignores `rax`.

mod encode;

use std::{alloc::Allocator, collections::HashMap, ops::Range};

use crate::{
    arch::encode::Xmm,
    xralloc2::{Class, Location, Reg as Preg, Xralloc2},
};
use encode::{
    Cond, Insn, R8, R9, R10, R11, R12, R13, R14, R15, RAX, RBP, RBX, RCX, RDI, RDX, RSI, RSP, Reg,
    patch_rel32,
};
use purple_garden_ir::{self as ir, BinOp, UnOp};
use purple_garden_runtime::{BuiltinFn, Value, string};

/// Bail out of [`compile_func`] (returning `None`) and, under the `trace`
/// feature, log why. The reason is only formatted inside `trace!`, so it costs
/// nothing when the feature is off; the whole diagnostic is trace-guarded.
macro_rules! skip {
    ($func:expr, $($reason:tt)*) => {{
        purple_garden_shared::trace!(
            "[jit::x86] skipped {}: {}",
            $func.name,
            format_args!($($reason)*)
        );
        return None;
    }};
}

/// `rbx` holds `*mut Vm` == `&vm.r[0]`, base for slot loads/stores. Callee saved, so Rust keeps
/// it across calls, `enter_native` sets it.
const VM: Reg = RBX;
/// Never holds a value: breaks edge-move cycles and holds helper addresses.
const GPR_SCRATCH: Reg = RAX;
/// Native functions take argument i in `ARG_GPRS[i]`, or in `xmm{i}` if it is a Double, and
/// return in `rax` or `xmm0`, see `enter_native`. Arguments past these stay in `vm.r[i]`, where
/// the interpreter put them; natives don't pass them to each other yet.
const ARG_GPRS: [Reg; 6] = [RSI, RDX, RCX, R8, R9, R10];

/// A builtin's native entry takes its arguments after the vm in `rdi`: these in order for
/// everything but Doubles, which take `xmm0..NATIVE_ENTRY_XMMS`.
const NATIVE_ENTRY_GPRS: [Reg; 5] = [RSI, RDX, RCX, R8, R9];
const NATIVE_ENTRY_XMMS: u8 = 8;

/// Allocatable GPRs, popped from the back: caller-saved first so leaf functions dont require a
/// prologue, `rcx`/`rdx` last since `idiv` clobbers them.
const GPR_POOL: &[u8] = &[
    RBP.0, R12.0, R13.0, R14.0, R15.0, RDI.0, RDX.0, RCX.0, R11.0, R10.0, R9.0, R8.0, RSI.0,
];
/// [`GPR_POOL`] without [`GPR_TEMPS`], for functions that spill.
const GPR_POOL_SPILLING: &[u8] = &[
    RBP.0, R12.0, R13.0, R14.0, R15.0, RDX.0, RCX.0, R10.0, R9.0, R8.0, RSI.0,
];
/// A value living in a slot is reloaded into these for the instruction reading it, a dst in a slot
/// is computed in the first and stored after. Only functions that spill give them up, see
/// [`Lowering::lower`]. Neither is an argument register nor touched by `idiv`.
const GPR_TEMPS: [Reg; 2] = [R11, RDI];
/// No frame pointer is kept, `rbp` is just another callee saved register.
const GPR_CALLEE_SAVED: &[Reg] = &[RBP, R12, R13, R14, R15];
/// Allocatable registers a SysV helper call clobbers.
const GPR_CALLER_SAVED: &[Reg] = &[RCX, RDX, RSI, RDI, R8, R9, R10, R11];
/// A helper call clobbers every xmm on top: SysV has no callee-saved fpr.
const CALL_CLOBBERS: Clobbers = Clobbers {
    gprs: GPR_CALLER_SAVED,
    fprs: true,
};

/// Allocatable XMM regs, also popped from the back; none are callee saved
const XMM_POOL: &[u8] = &[14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0];
/// scratch reg, not in pool, cycles and helper loc
const XMM_SCRATCH: Xmm = Xmm(15);
/// [`XMM_POOL`] without [`XMM_TEMPS`].
const XMM_POOL_SPILLING: &[u8] = &[12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0];
/// The [`GPR_TEMPS`] of doubles.
const XMM_TEMPS: [Xmm; 2] = [Xmm(14), Xmm(13)];

/// Compile one IR function into x86-64 machine code, returning `None` if unsupported constructs are
/// included
pub fn compile_func<'ir, F: Allocator, S: Allocator + Clone, N: Allocator>(
    func: &ir::Func<'ir, F>,
    out: &mut Vec<u8, S>,
    liveness: &[(u32, u32)],
    globals: &HashMap<ir::Const<'ir>, u32>,
    strings: &[Value],
    natives: &crate::Natives<N>,
    at: usize,
    ra: &mut Xralloc2<S>,
    buffers: &mut Scratch<S>,
) -> Option<()> {
    let Some(entry) = func.blocks.iter().find(|b| !b.tombstone).map(|b| b.id) else {
        skip!(func, "empty function");
    };

    let calls = Lowering::new(
        func, liveness, globals, strings, natives, ra, buffers, entry,
    )
    .lower()?;

    // The body is emitted first since the callee-saved registers it uses are
    // only known afterwards. Wrap it into a frame:
    //
    //     push <saved>     ; callee-saved regs the body used, the interpreter
    //                      ; calls us as a C function and expects them intact
    //     sub rsp, 8*n     ; slot k at [rsp+8k], plus a pad if needed for alignment
    //     <body>           ; returns jump to the epilogue below
    //     add rsp, 8*n
    //     pop <saved>      ; reverse order
    //     ret
    let saved = GPR_CALLEE_SAVED
        .iter()
        .copied()
        .filter(|reg| ra.used(Preg::Gpr(reg.0)));

    // SysV wants rsp 16-byte aligned at every `call`. On entry rsp is 8 past
    // that (our caller pushed the return address) and every push flips it, so
    // an even number of pushes leaves it misaligned. Only matters if the body
    // calls helpers.
    let slots = u32::from(ra.slots());
    let pad = calls && (saved.clone().count() as u32 + slots).is_multiple_of(2);
    let frame = 8 * (slots + u32::from(pad));
    let frameless = frame == 0 && saved.clone().next().is_none();

    if frameless {
        // The epilogue is a bare `ret`, so each `jmp epilogue` (e9 + rel32) in
        // the body becomes `ret` (c3). The rel32 bytes become int3 (cc): nothing
        // executes past a ret and nothing jumps into them, and rewriting in
        // place keeps every other offset valid.
        for patch in buffers
            .patches
            .iter()
            .filter(|p| p.target == Target::Epilogue)
        {
            debug_assert_eq!(
                buffers.body[patch.rel - 1],
                0xe9,
                "epilogue jumps are unconditional"
            );
            buffers.body[patch.rel - 1] = 0xc3;
            buffers.body[patch.rel..patch.rel + 4].fill(0xcc);
        }
    }

    out.reserve(buffers.body.len() + 32);
    let start = out.len();
    for reg in saved.clone() {
        Insn::Push { reg }.encode(out);
    }
    if frame > 0 {
        Insn::SubImm {
            dst: RSP,
            imm: frame as i32,
        }
        .encode(out);
    }
    let body_at = out.len();
    out.extend_from_slice(&buffers.body);
    for table in &mut buffers.tables {
        *table = table.start + body_at - start..table.end + body_at - start;
    }
    // Self calls take the address of the function's first byte, in front of
    // the prologue that is only laid out now.
    for &rel in &buffers.self_refs {
        patch_rel32(out, body_at + rel, body_at + rel + 4, start).expect("within the function");
    }
    // Calls to other natives, as distances from where this function lands at `at`.
    let patch_native = |out: &mut Vec<u8, S>, rel: usize, target: usize| {
        let from = at + (rel + 4 - start);
        let disp = i32::try_from(target as isize - from as isize).expect("natives share an arena");
        out[rel..rel + 4].copy_from_slice(&disp.to_le_bytes());
    };
    for &(rel, target) in &buffers.native_refs {
        patch_native(out, body_at + rel, target);
    }
    let epilogue = |out: &mut Vec<u8, S>| {
        if frame > 0 {
            Insn::AddImm {
                dst: RSP,
                imm: frame as i32,
            }
            .encode(out);
        }
        for reg in saved.clone().rev() {
            Insn::Pop { reg }.encode(out);
        }
    };
    epilogue(out);
    Insn::Ret.encode(out);
    // A tail call leaves the frame like a return, then jumps to the callee, which returns to this
    // function's caller. Without a frame that is the jump itself, otherwise each callee gets an
    // epilogue ending in the jump.
    buffers
        .tail_refs
        .sort_unstable_by_key(|&(_, target)| target);
    for refs in buffers.tail_refs.chunk_by(|a, b| a.1 == b.1) {
        let target = refs[0].1;
        if frameless {
            for &(rel, _) in refs {
                patch_native(out, body_at + rel, target);
            }
            continue;
        }
        let stub = out.len();
        epilogue(out);
        let rel = encode::jump(out, Cond::Always);
        patch_native(out, rel, target);
        for &(rel, _) in refs {
            patch_rel32(out, body_at + rel, body_at + rel + 4, stub).expect("within the function");
        }
    }

    purple_garden_shared::trace!("[jit::x86] compiled {} ({} bytes)", func.name, out.len());
    Some(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Block(ir::Id),
    /// The shared epilogue appended after the body.
    Epilogue,
    /// A tail call to the native function at this address, resolved once the frame is known.
    Tail(usize),
}

#[derive(Debug, Clone, Copy)]
struct Patch {
    /// Offset of the rel32 displacement in the body.
    rel: usize,
    target: Target,
    /// Body offset the displacement counts from: the end of the instruction
    /// for jumps, the table start for switch table entries.
    from: usize,
}

/// Reusable lowering allocation storage
#[derive(Debug, Clone)]
pub struct Scratch<S: Allocator> {
    body: Vec<u8, S>,
    block_offsets: Vec<usize, S>,
    patches: Vec<Patch, S>,
    move_pairs: Vec<(Location, Location), S>,
    /// `(reg, copy)` pairs preserved across a clobbering instruction.
    saves: Vec<(Preg, Location), S>,
    /// Register hint per value, a block param gets its edge arg's register.
    hints: Vec<Option<Preg>, S>,
    /// Class per value, a block param gets its edge arg's.
    classes: Vec<Class, S>,
    /// The `(reg, copy)` pairs [`Lowering::prealloc`] planned, per row from `copies_at[row]` to
    /// `copies_at[row + 1]`.
    copies: Vec<(Preg, Location), S>,
    copies_at: Vec<u32, S>,
    /// Body offsets of the rel32s (self calls) that address the function's own start.
    self_refs: Vec<usize, S>,
    /// Body offsets of the rel32s (tail calls) and the native function each jumps to.
    tail_refs: Vec<(usize, usize), S>,
    /// Body offsets of the rel32s (native calls) and the native function each calls.
    native_refs: Vec<(usize, usize), S>,
    /// Jump and lookup tables inlined into the code, as body offsets until the function is laid
    /// out, then as offsets from its first byte.
    tables: Vec<Range<usize>, S>,
}

impl<S: Allocator + Clone> Scratch<S> {
    pub fn tables(&self) -> &[Range<usize>] {
        &self.tables
    }

    pub fn new_in(scratch: S) -> Self {
        Self {
            body: Vec::new_in(scratch.clone()),
            block_offsets: Vec::new_in(scratch.clone()),
            patches: Vec::new_in(scratch.clone()),
            move_pairs: Vec::new_in(scratch.clone()),
            saves: Vec::new_in(scratch.clone()),
            hints: Vec::new_in(scratch.clone()),
            classes: Vec::new_in(scratch.clone()),
            copies: Vec::new_in(scratch.clone()),
            copies_at: Vec::new_in(scratch.clone()),
            self_refs: Vec::new_in(scratch.clone()),
            tail_refs: Vec::new_in(scratch.clone()),
            native_refs: Vec::new_in(scratch.clone()),
            tables: Vec::new_in(scratch),
        }
    }
}

/// Target of a native to native call, see [`Lowering::call_native`].
#[derive(Clone, Copy)]
enum Callee {
    /// A natively compiled function.
    Native(BuiltinFn),
    /// The function being compiled, whose address isn't known yet.
    This,
}

/// The registers an instruction overwrites, per class: a GPR list and
/// whether the whole fpr file goes.
#[derive(Clone, Copy)]
struct Clobbers {
    gprs: &'static [Reg],
    fprs: bool,
}

impl Clobbers {
    fn contains(self, reg: Preg) -> bool {
        match reg {
            Preg::Gpr(num) => self.gprs.contains(&Reg(num)),
            Preg::Fpr(_) => self.fprs,
        }
    }
}

/// Argument of a SysV helper call.
#[derive(Clone, Copy)]
enum AbiArg {
    Reg(Reg),
    Imm(u64),
}

/// The `n`th temp of `class`, see [`GPR_TEMPS`].
fn temp(class: Class, n: usize) -> Preg {
    match class {
        Class::Gpr => Preg::Gpr(GPR_TEMPS[n].0),
        Class::Fpr => Preg::Fpr(XMM_TEMPS[n].0),
    }
}

/// Where a native function takes its `i`th argument of `class`.
fn arg_reg(class: Class, i: usize) -> Preg {
    match class {
        Class::Gpr => Preg::Gpr(ARG_GPRS[i].0),
        Class::Fpr => Preg::Fpr(i as u8),
    }
}

/// Divisors [`Lowering::div`] leaves to shifts.
fn pow2(imm: i32) -> bool {
    imm > 1 && imm.count_ones() == 1
}

#[derive(Clone, Copy)]
enum Divisor {
    Reg(Reg),
    Imm(i32),
}

/// `dst <op>= src` for the integer ops x86 encodes two-operand.
fn arith(op: BinOp, dst: Reg, src: Reg) -> Insn {
    match op {
        BinOp::IAdd => Insn::Add { dst, src },
        BinOp::ISub => Insn::Sub { dst, src },
        BinOp::IMul => Insn::Imul { dst, src },
        BinOp::IAnd => Insn::And { dst, src },
        BinOp::IOr => Insn::Or { dst, src },
        BinOp::IXor => Insn::Xor { dst, src },
        _ => unreachable!(
            "only IAdd, ISub, IMul, IAnd, IOr and IXor lower to two-operand arithmetic"
        ),
    }
}

/// Mark the function as not compilable. [`Lowering::lower`] stops after the
/// current instruction; until then lowering continues on placeholder
/// registers and the output is discarded.
macro_rules! bail {
    ($self:expr, $($reason:tt)*) => {{
        purple_garden_shared::trace!(
            "[jit::x86] skipped {}: {}",
            $self.func.name,
            format_args!($($reason)*)
        );
        $self.unsupported = true;
    }};
}

/// Lowers one function, assigning registers before it emits.
///
/// Registers come from [`Xralloc2`] in a pre-pass over the whole function and stay
/// fixed for the value's whole liveness interval, so every CFG edge agrees on
/// where a value lives.
struct Lowering<'a, 'ir, F: Allocator, S: Allocator + Clone, N: Allocator> {
    func: &'a ir::Func<'ir, F>,
    liveness: &'a [(u32, u32)],
    /// `vm.globals` slot of each constant.
    globals: &'a HashMap<ir::Const<'ir>, u32>,
    /// `strings[slot]` is the final address of the string constant in `slot`.
    strings: &'a [Value],
    /// Entry points of the functions already compiled natively.
    natives: &'a crate::Natives<N>,
    ra: &'a mut Xralloc2<S>,
    out: &'a mut Vec<u8, S>,
    entry: ir::Id,
    /// The block being lowered, for the facts that only hold inside it.
    block: ir::Id,
    /// Walks in lockstep with [`ir::Func::live_set_into`]: one unit per block
    /// header, instruction and terminator, uses and defs both on `pos`.
    pos: u32,
    /// Set once a helper or native function is called; the prologue then aligns the stack.
    calls: bool,
    unsupported: bool,
    /// Double arithmetic is only lowered to the three-operand AVX forms.
    avx: bool,
    /// `block_offsets[block_id]` is the body offset of the block's first byte,
    /// `usize::MAX` for blocks not emitted.
    block_offsets: &'a mut Vec<usize, S>,
    patches: &'a mut Vec<Patch, S>,
    /// Reusable `(src, dst)` buffer for the parallel-move resolver in
    /// [`Lowering::edge_moves`].
    move_pairs: &'a mut Vec<(Location, Location), S>,
    /// Reusable `(reg, copy)` buffer of [`Lowering::save_clobbered`].
    saves: &'a mut Vec<(Preg, Location), S>,
    hints: &'a mut Vec<Option<Preg>, S>,
    classes: &'a mut Vec<Class, S>,
    /// [`GPR_TEMPS`] and [`XMM_TEMPS`] the current instruction took.
    temps: [usize; 2],
    copies: &'a mut Vec<(Preg, Location), S>,
    copies_at: &'a mut Vec<u32, S>,
    self_refs: &'a mut Vec<usize, S>,
    tail_refs: &'a mut Vec<(usize, usize), S>,
    native_refs: &'a mut Vec<(usize, usize), S>,
    tables: &'a mut Vec<Range<usize>, S>,
}

impl<'a, 'ir, F: Allocator, S: Allocator + Clone, N: Allocator> Lowering<'a, 'ir, F, S, N> {
    fn new(
        func: &'a ir::Func<'ir, F>,
        liveness: &'a [(u32, u32)],
        globals: &'a HashMap<ir::Const<'ir>, u32>,
        strings: &'a [Value],
        natives: &'a crate::Natives<N>,
        ra: &'a mut Xralloc2<S>,
        buffers: &'a mut Scratch<S>,
        entry: ir::Id,
    ) -> Self {
        let Scratch {
            body,
            block_offsets,
            patches,
            move_pairs,
            saves,
            hints,
            classes,
            copies,
            copies_at,
            self_refs,
            tail_refs,
            native_refs,
            tables,
        } = buffers;
        body.clear();
        block_offsets.clear();
        block_offsets.resize(func.blocks.len(), usize::MAX);
        patches.clear();
        self_refs.clear();
        tail_refs.clear();
        native_refs.clear();
        tables.clear();
        Self {
            func,
            liveness,
            globals,
            strings,
            natives,
            ra,
            out: body,
            entry,
            block: entry,
            pos: 0,
            calls: false,
            temps: [0; 2],
            unsupported: false,
            avx: std::arch::is_x86_feature_detected!("avx"),
            block_offsets,
            patches,
            move_pairs,
            saves,
            hints,
            classes,
            copies,
            copies_at,
            self_refs,
            tail_refs,
            native_refs,
            tables,
        }
    }

    fn scratch(&self) -> S {
        self.out.allocator().clone()
    }

    /// Emit the body, ending where the epilogue is appended. Returns whether
    /// it calls helpers, `None` if the function can't be compiled.
    fn lower(mut self) -> Option<bool> {
        self.prealloc(GPR_POOL, XMM_POOL);
        // values in slots need temps to be read and written, only a function that spills pays for
        // them, with registers it was short of anyway
        if !self.unsupported && self.ra.slots() > 0 {
            self.prealloc(GPR_POOL_SPILLING, XMM_POOL_SPILLING);
        }
        if self.unsupported {
            return None;
        }

        // Native calls recurse on the machine stack: trap like the interpreter
        // before running off its end.
        // Functions are defined before use, so only a function calling itself
        // can recurse, a tail call to itself is a loop.
        let recurses = self
            .func
            .blocks
            .iter()
            .filter(|b| !b.tombstone)
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i, ir::Instr::Call { func, .. } if *func == self.func.id));
        if recurses {
            self.emit(Insn::CmpMem {
                reg: RSP,
                base: VM,
                offset: purple_garden_runtime::NATIVE_STACK_LIMIT_OFFSET as u32,
            });
            let fine = encode::jump(self.out, Cond::AboveEq);
            self.call(
                purple_garden_runtime::jit_trap_stack_overflow as *const () as u64,
                &[AbiArg::Reg(VM)],
                None,
            );
            let resume = self.out.len();
            patch_rel32(self.out, fine, fine + 4, resume).expect("short forward jump");
        }

        for (i, param) in self.func.params.iter().enumerate() {
            if self
                .liveness
                .get(param.id.0 as usize)
                .is_none_or(|&(start, _)| start == u32::MAX)
            {
                continue;
            }
            if i >= ARG_GPRS.len() {
                // the argument registers are all taken by now, this can't clobber one
                let offset = 8 * i as u32;
                self.temps = [0; 2];
                let dst = self.def_id(param.id);
                self.emit(match dst {
                    Preg::Gpr(dst) => Insn::LoadMem {
                        dst: Reg(dst),
                        base: VM,
                        offset,
                    },
                    Preg::Fpr(dst) => Insn::LoadMemSd {
                        dst: Xmm(dst),
                        base: VM,
                        offset,
                    },
                });
                self.spill(param.id);
            }
        }

        let blocks = &self.func.blocks;
        for (idx, block) in blocks.iter().enumerate() {
            if block.tombstone {
                continue;
            }
            self.block_offsets[block.id.0 as usize] = self.out.len();
            self.block = block.id;
            self.pos += 1;

            for instruction in &block.instructions {
                self.temps = [0; 2];
                self.instr(instruction);
                if self.unsupported {
                    return None;
                }
                if let Some(dst) = ir::Func::def_of(instruction) {
                    self.spill(dst);
                }
                self.pos += 1;
            }

            let next = blocks[idx + 1..]
                .iter()
                .find(|b| !b.tombstone)
                .map(|b| b.id);
            self.temps = [0; 2];
            self.term(block.term.as_ref(), next);
            if self.unsupported {
                return None;
            }
            self.pos += 1;
        }

        self.patch_jumps();
        (!self.unsupported).then_some(self.calls)
    }

    fn emit(&mut self, insn: Insn) {
        insn.encode(self.out);
    }

    #[inline(always)]
    fn gpr(&mut self, reg: Preg) -> Reg {
        if let Preg::Gpr(gpr) = reg {
            Reg(gpr)
        } else {
            bail!(self, "gpr called on a value thats not a gpr");
            GPR_SCRATCH
        }
    }

    #[inline(always)]
    fn xmm(&mut self, reg: Preg) -> Xmm {
        if let Preg::Fpr(fpr) = reg {
            Xmm(fpr)
        } else {
            bail!(self, "xmm called on a value thats not a fpr");
            XMM_SCRATCH
        }
    }

    /// Give every value its register before lowering, walking the rows of
    /// [`ir::Func::live_set_into`]. An instruction overwriting registers besides its dst gets
    /// free registers planned for the values living there, see [`Lowering::save_clobbered`].
    fn prealloc(&mut self, gprs: &[u8], fprs: &[u8]) {
        let func = self.func;
        purple_garden_shared::trace!(
            "[jit::x86] prealloc {} with {} gprs, {} fprs",
            func.name,
            gprs.len(),
            fprs.len()
        );
        self.ra.reset(self.liveness.len(), gprs, fprs);
        self.hints.clear();
        self.hints.resize(self.liveness.len(), None);
        self.classes.clear();
        self.classes.resize(self.liveness.len(), Class::Gpr);
        self.copies.clear();
        self.copies_at.clear();

        for (i, param) in func.params.iter().enumerate() {
            let Some(&(_, last_use)) = self
                .liveness
                .get(param.id.0 as usize)
                .filter(|&&(start, _)| start != u32::MAX)
            else {
                continue;
            };
            let class = Class::from(&param.ty);
            self.classes[param.id.0 as usize] = class;
            if i >= ARG_GPRS.len() {
                self.alloc(param.id, class, 0);
                continue;
            }
            let reg = match class {
                Class::Gpr => Preg::Gpr(ARG_GPRS[i].0),
                Class::Fpr => Preg::Fpr(i as u8),
            };
            if self.ra.try_alloc_at(param.id, reg, last_use).is_none() {
                bail!(self, "param register {reg:?} is taken");
                return;
            }
        }

        let mut pos = 0;
        for (idx, block) in func.blocks.iter().enumerate() {
            if block.tombstone {
                continue;
            }
            self.copies_at.push(self.copies.len() as u32);
            for &param in func.params(block.params) {
                self.alloc(param, self.classes[param.0 as usize], pos);
            }
            self.hint_uses(idx);
            pos += 1;

            for instruction in &block.instructions {
                self.copies_at.push(self.copies.len() as u32);
                self.plan(instruction, pos);
                pos += 1;
            }

            self.copies_at.push(self.copies.len() as u32);
            let mut hint = |(target, params): (ir::Id, ir::ParamsId)| {
                if (target.0 as usize) <= idx {
                    return;
                }
                let dst = func.params(func.blocks[target.0 as usize].params);
                for (&src, &dst) in func.params(params).iter().zip(dst) {
                    self.classes[dst.0 as usize] = self.classes[src.0 as usize];
                    if let Some(reg) = self.ra.reg(src) {
                        self.hints[dst.0 as usize] = Some(reg);
                    }
                }
            };
            match &block.term {
                Some(ir::Terminator::Jump { id, params, .. }) => hint((*id, *params)),
                Some(
                    ir::Terminator::Branch { yes, no, .. }
                    | ir::Terminator::BranchCmpImm { yes, no, .. }
                    | ir::Terminator::BranchCmp { yes, no, .. },
                ) => {
                    hint(*no);
                    hint(*yes);
                }
                Some(ir::Terminator::Switch { cases, default, .. }) => {
                    for case in func.cases(*cases) {
                        hint(case.target);
                    }
                    hint(*default);
                }
                Some(ir::Terminator::Return { .. } | ir::Terminator::Tail { .. }) | None => {}
            }
            pos += 1;
        }
        self.copies_at.push(self.copies.len() as u32);
        purple_garden_shared::trace!(
            "[jit::x86] prealloc {}: {} slots",
            func.name,
            self.ra.slots()
        );
    }

    /// Assign `i`'s dst at `pos`, after planning the copies of what lowering `i` clobbers. A
    /// call's copies are free again once it returned, so its dst may take one.
    fn plan(&mut self, i: &ir::Instr<'ir>, pos: u32) {
        const IDIV: Clobbers = Clobbers {
            gprs: &[RDX],
            fprs: false,
        };
        // a divisor in rdx, or an immediate one, is staged in rcx
        const IDIV_STAGED: Clobbers = Clobbers {
            gprs: &[RCX, RDX],
            fprs: false,
        };
        let (clobbers, call) = match i {
            ir::Instr::Call { .. } | ir::Instr::Sys { .. } => (Some(CALL_CLOBBERS), true),
            ir::Instr::Alloc { .. } => (Some(CALL_CLOBBERS), false),
            ir::Instr::Bin {
                op: BinOp::IDiv | BinOp::IMod,
                rhs,
                ..
            } if self.ra.reg(*rhs) == Some(Preg::Gpr(RDX.0)) => (Some(IDIV_STAGED), false),
            ir::Instr::Bin {
                op: BinOp::IDiv | BinOp::IMod,
                ..
            } => (Some(IDIV), false),
            ir::Instr::BinImm {
                op: BinOp::IDiv | BinOp::IMod,
                imm,
                ..
            } if *imm != 0 && !pow2(*imm) => (Some(IDIV_STAGED), false),
            _ => (None, false),
        };

        let start = self.copies.len();
        if let Some(clobbers) = clobbers {
            // the parallel move reads a call's args before writing a copy over one dying there
            if call {
                self.ra.expire(pos);
            }
            self.reserve_copies(clobbers, pos);
        }
        if call {
            self.give_back_copies(start);
        }
        // a Void value is never read, see the Assert lowering
        let dst = ir::Func::typed_def_of(i).filter(|_| {
            !matches!(
                i,
                ir::Instr::Un {
                    op: UnOp::Assert,
                    ..
                }
            )
        });
        if let Some(dst) = dst {
            self.alloc(dst.id, Class::from(&dst.ty), pos);
        }
        if !call {
            self.give_back_copies(start);
        }
    }

    /// Hint the values block `idx` passes on at where they are passed: call args at their argument
    /// registers, a returned double at xmm0, back edge args at the params they move into, which have
    /// their registers already. A forward edge's params take their args' registers instead.
    fn hint_uses(&mut self, idx: usize) {
        let func = self.func;
        let block = &func.blocks[idx];
        for dst in block.instructions.iter().filter_map(ir::Func::typed_def_of) {
            self.classes[dst.id.0 as usize] = Class::from(&dst.ty);
        }
        for instr in &block.instructions {
            match instr {
                ir::Instr::Call { args, .. } => self.hint_args(func.params(*args)),
                ir::Instr::Sys { fun, args, .. }
                    if fun.intrinsic.is_none() && fun.native.is_some() =>
                {
                    let (mut gprs, mut xmms) = (NATIVE_ENTRY_GPRS.iter(), 0..NATIVE_ENTRY_XMMS);
                    for (&arg, ty) in func.params(*args).iter().zip(fun.args) {
                        let reg = match Class::from(ty) {
                            Class::Gpr => gprs.next().map(|r| Preg::Gpr(r.0)),
                            Class::Fpr => xmms.next().map(Preg::Fpr),
                        };
                        if let Some(reg) = reg {
                            self.hint(arg, reg);
                        }
                    }
                }
                _ => {}
            }
        }

        let back_edge = |this: &mut Self, (target, args): (ir::Id, ir::ParamsId)| {
            if target.0 as usize > idx {
                return;
            }
            let params = func.params(func.blocks[target.0 as usize].params);
            for (&arg, &param) in func.params(args).iter().zip(params) {
                if let Some(reg) = this.ra.reg(param) {
                    this.hint(arg, reg);
                }
            }
        };
        match &block.term {
            Some(ir::Terminator::Return {
                value: Some(value), ..
            }) => self.hint(*value, Preg::Fpr(0)),
            Some(ir::Terminator::Jump { id, params, .. }) => back_edge(self, (*id, *params)),
            Some(
                ir::Terminator::Branch { yes, no, .. }
                | ir::Terminator::BranchCmpImm { yes, no, .. }
                | ir::Terminator::BranchCmp { yes, no, .. },
            ) => {
                back_edge(self, *yes);
                back_edge(self, *no);
            }
            Some(ir::Terminator::Switch { cases, default, .. }) => {
                for case in func.cases(*cases) {
                    back_edge(self, case.target);
                }
                back_edge(self, *default);
            }
            Some(ir::Terminator::Tail {
                func: callee, args, ..
            }) if *callee == func.id => {
                back_edge(self, (self.entry, *args));
            }
            Some(ir::Terminator::Tail { args, .. }) => self.hint_args(func.params(*args)),
            Some(ir::Terminator::Return { value: None, .. }) | None => {}
        }
    }

    /// Hint `args` at the registers a native function takes them in.
    fn hint_args(&mut self, args: &[ir::Id]) {
        for (i, &arg) in args.iter().enumerate().take(ARG_GPRS.len()) {
            self.hint(arg, arg_reg(self.classes[arg.0 as usize], i));
        }
    }

    /// The first hint of a value wins, and only one of its class.
    fn hint(&mut self, id: ir::Id, reg: Preg) {
        let hint = &mut self.hints[id.0 as usize];
        if hint.is_none() && reg.class() == self.classes[id.0 as usize] {
            *hint = Some(reg);
        }
    }

    fn alloc(&mut self, id: ir::Id, class: Class, at: u32) {
        let last_use = self.liveness[id.0 as usize].1;
        self.classes[id.0 as usize] = class;
        match self.hints[id.0 as usize] {
            Some(hint) => self.ra.alloc_with_hint(id, class, at, last_use, hint),
            None => self.ra.alloc(id, class, at, last_use),
        };
    }

    fn give_back_copies(&mut self, start: usize) {
        for &(_, copy) in &self.copies[start..] {
            match copy {
                Location::Reg(copy) => self.ra.give_back(copy),
                Location::Stack(slot) => self.ra.give_back_slot(slot),
            }
        }
    }

    fn def(&mut self, dst: &ir::TypeId<'ir>) -> Preg {
        self.def_id(dst.id)
    }

    /// The register [`Lowering::prealloc`] gave `id`, the first temp if it lives in a slot:
    /// [`Lowering::spill`] stores it there after the instruction.
    fn def_id(&mut self, id: ir::Id) -> Preg {
        match self.ra.location(id) {
            Some(Location::Reg(reg)) => reg,
            Some(Location::Stack(_)) => temp(self.classes[id.0 as usize], 0),
            None => {
                bail!(self, "%v{} has no register", id.0);
                Preg::Gpr(GPR_SCRATCH.0)
            }
        }
    }

    /// The register holding `id`, a value living in a slot is loaded into the next free temp.
    fn ensure_register(&mut self, id: ir::Id) -> Preg {
        match self.ra.location(id) {
            Some(Location::Reg(reg)) => reg,
            Some(Location::Stack(slot)) => {
                let class = self.classes[id.0 as usize];
                let taken = &mut self.temps[class as usize];
                if *taken == GPR_TEMPS.len() {
                    bail!(self, "more operands in slots than temps at %v{}", id.0);
                    return Preg::Gpr(GPR_SCRATCH.0);
                }
                let reg = temp(class, *taken);
                *taken += 1;
                self.load_slot(reg, slot);
                reg
            }
            None => {
                bail!(self, "use of %v{} before its definition", id.0);
                Preg::Gpr(GPR_SCRATCH.0)
            }
        }
    }

    /// The location of `id`, which must have one.
    fn location(&mut self, id: ir::Id) -> Location {
        match self.ra.location(id) {
            Some(location) => location,
            None => {
                bail!(self, "use of %v{} before its definition", id.0);
                Location::Reg(Preg::Gpr(GPR_SCRATCH.0))
            }
        }
    }

    /// Store `id` into its slot from the temp [`Lowering::def_id`] computed it in, if it has one.
    fn spill(&mut self, id: ir::Id) {
        if let Some(Location::Stack(slot)) = self.ra.location(id) {
            self.store_slot(slot, temp(self.classes[id.0 as usize], 0));
        }
    }

    fn load_slot(&mut self, dst: Preg, slot: u16) {
        let offset = 8 * u32::from(slot);
        self.emit(match dst {
            Preg::Gpr(dst) => Insn::LoadMem {
                dst: Reg(dst),
                base: RSP,
                offset,
            },
            Preg::Fpr(dst) => Insn::LoadMemSd {
                dst: Xmm(dst),
                base: RSP,
                offset,
            },
        });
    }

    fn store_slot(&mut self, slot: u16, src: Preg) {
        let offset = 8 * u32::from(slot);
        self.emit(match src {
            Preg::Gpr(src) => Insn::StoreMem {
                base: RSP,
                offset,
                src: Reg(src),
            },
            Preg::Fpr(src) => Insn::StoreMemSd {
                base: RSP,
                offset,
                src: Xmm(src),
            },
        });
    }

    fn instr(&mut self, i: &ir::Instr<'ir>) {
        match i {
            ir::Instr::Noop => {}
            ir::Instr::Store {
                src, base, offset, ..
            } if *offset <= i32::MAX as u32 => {
                let src = self.ensure_register(*src);
                let base = self.ensure_register(*base);
                let base = self.gpr(base);
                let offset = *offset;
                self.emit(match src {
                    Preg::Gpr(src) => Insn::StoreMem {
                        base,
                        offset,
                        src: Reg(src),
                    },
                    Preg::Fpr(src) => Insn::StoreMemSd {
                        base,
                        offset,
                        src: Xmm(src),
                    },
                });
            }
            ir::Instr::Load {
                dst, base, offset, ..
            } if *offset <= i32::MAX as u32 => {
                let base = self.ensure_register(*base);
                let base = self.gpr(base);
                let offset = *offset;
                let dst = self.def(dst);
                self.emit(match dst {
                    Preg::Gpr(dst) => Insn::LoadMem {
                        dst: Reg(dst),
                        base,
                        offset,
                    },
                    Preg::Fpr(dst) => Insn::LoadMemSd {
                        dst: Xmm(dst),
                        base,
                        offset,
                    },
                });
            }
            ir::Instr::AddrOf {
                dst, base, offset, ..
            } if *offset <= i32::MAX as u32 => {
                let base = self.ensure_register(*base);
                let base = self.gpr(base);
                let dst = self.def(dst);
                let dst = self.gpr(dst);
                self.emit(Insn::LeaMem {
                    dst,
                    base,
                    offset: *offset,
                });
            }
            ir::Instr::Alloc {
                dst: ir::TypeId { id, ty },
                layout,
                ..
            } => {
                let Some(kind) = purple_garden_runtime::AllocType::from_ty(ty) else {
                    bail!(self, "unsupported allocation type");
                    return;
                };
                let saves = self.save_clobbered();
                let dst = self.def_id(*id);
                let dst = self.gpr(dst);
                self.call(
                    purple_garden_runtime::jit_alloc as *const () as u64,
                    &[
                        AbiArg::Reg(VM),
                        AbiArg::Imm(kind as u64),
                        AbiArg::Imm(layout.size() as u64),
                        AbiArg::Imm(layout.align() as u64),
                    ],
                    Some(dst),
                );
                self.restore(saves);
            }
            ir::Instr::LoadConst {
                dst,
                value: value @ ir::Const::Str(_),
                ..
            } => {
                let Some(&value) = self
                    .globals
                    .get(value)
                    .and_then(|&slot| self.strings.get(slot as usize))
                else {
                    bail!(self, "string constant missing from the const pool");
                    return;
                };
                let dst = self.def(dst);
                let dst = self.gpr(dst);
                self.emit(Insn::MovAbs { dst, imm: value.0 });
            }
            ir::Instr::LoadConst {
                dst,
                value: ir::Const::Double(bits),
                ..
            } => {
                // xmm doesnt have an immediate form:
                // so we move the bits in the GPR scratch.
                let dst = self.def(dst);
                let dst = self.xmm(dst);
                self.emit(Insn::MovAbs {
                    dst: GPR_SCRATCH,
                    imm: *bits,
                });
                self.emit(Insn::MovqToXmm {
                    dst,
                    src: GPR_SCRATCH,
                });
            }
            // a Void value is never read, whatever the register holds will do
            ir::Instr::LoadConst {
                dst,
                value: ir::Const::Undefined,
                ..
            } => {
                self.def(dst);
            }
            ir::Instr::LoadConst { dst, value, .. } => {
                let Some(imm) = (match value {
                    ir::Const::False => Some(0),
                    ir::Const::True => Some(1),
                    ir::Const::Int(i) => Some(*i),
                    _ => None,
                }) else {
                    bail!(self, "const is not a bool or i32");
                    return;
                };

                let dst = self.def(dst);
                let dst = self.gpr(dst);
                if imm > i32::MAX as i64 || imm < i32::MIN as i64 {
                    let imm = imm.cast_unsigned();
                    self.emit(Insn::MovAbs { dst, imm });
                } else {
                    self.emit(Insn::MovImm {
                        dst,
                        imm: imm as i32,
                    });
                }
            }
            // div defines dst itself, only after saving what idiv clobbers: a dst
            // defined before could sit in rcx/rdx and be restored over.
            ir::Instr::BinImm {
                op: op @ (BinOp::IDiv | BinOp::IMod),
                dst,
                lhs,
                imm,
                ..
            } if *imm != 0 && !pow2(*imm) => {
                let lhs = self.ensure_register(*lhs);
                let lhs = self.gpr(lhs);
                self.div(*op, dst.id, lhs, Divisor::Imm(*imm));
            }
            ir::Instr::BinImm {
                op, dst, lhs, imm, ..
            } => {
                let lhs = self.ensure_register(*lhs);
                let lhs = self.gpr(lhs);
                let imm = *imm;
                let dst = self.def(dst);
                let dst = self.gpr(dst);

                match op {
                    BinOp::IAdd | BinOp::ISub | BinOp::IAnd => {
                        if dst != lhs {
                            self.emit(Insn::Mov { dst, src: lhs });
                        }
                        self.emit(match op {
                            BinOp::IAdd => Insn::AddImm { dst, imm },
                            BinOp::ISub => Insn::SubImm { dst, imm },
                            _ => Insn::AndImm { dst, imm },
                        });
                    }
                    BinOp::IEq => {
                        if imm == 0 {
                            self.emit(Insn::Test { lhs, rhs: lhs });
                        } else {
                            self.emit(Insn::CmpImm { reg: lhs, imm });
                        }
                        // mov leaves the flags alone; sete only writes the low byte.
                        self.emit(Insn::MovImm { dst, imm: 0 });
                        self.emit(Insn::Sete { dst });
                    }
                    BinOp::ILt | BinOp::IGt => {
                        self.emit(Insn::CmpImm { reg: lhs, imm });
                        self.emit(Insn::MovImm { dst, imm: 0 });
                        self.emit(match op {
                            BinOp::ILt => Insn::Setl { dst },
                            _ => Insn::Setg { dst },
                        });
                    }
                    BinOp::IMul => {
                        self.emit(Insn::ImulImm { dst, src: lhs, imm });
                    }
                    BinOp::IDiv | BinOp::IMod if imm == 0 => {
                        self.trap_div_zero();
                    }
                    BinOp::IDiv | BinOp::IMod => {
                        // Shifts round down, division truncates: bias a negative x by imm - 1
                        // to round it toward zero instead.
                        let k = imm.trailing_zeros() as u8;
                        self.emit(Insn::Mov {
                            dst: GPR_SCRATCH,
                            src: lhs,
                        });
                        self.emit(Insn::SarImm {
                            dst: GPR_SCRATCH,
                            imm: 63,
                        });
                        self.emit(Insn::ShrImm {
                            dst: GPR_SCRATCH,
                            imm: 64 - k,
                        });
                        self.emit(Insn::Add {
                            dst: GPR_SCRATCH,
                            src: lhs,
                        });
                        if *op == BinOp::IDiv {
                            self.emit(Insn::SarImm {
                                dst: GPR_SCRATCH,
                                imm: k,
                            });
                            self.emit(Insn::Mov {
                                dst,
                                src: GPR_SCRATCH,
                            });
                        } else {
                            // x - x / imm * imm
                            self.emit(Insn::AndImm {
                                dst: GPR_SCRATCH,
                                imm: -imm,
                            });
                            if dst != lhs {
                                self.emit(Insn::Mov { dst, src: lhs });
                            }
                            self.emit(Insn::Sub {
                                dst,
                                src: GPR_SCRATCH,
                            });
                        }
                    }
                    _ => bail!(self, "unsupported binimm op {op:?}"),
                }
            }
            ir::Instr::Bin {
                op: op @ (BinOp::DAdd | BinOp::DSub | BinOp::DMul | BinOp::DDiv),
                dst,
                lhs,
                rhs,
                ..
            } => {
                if !self.avx {
                    bail!(
                        self,
                        "double bin ops need avx, your shitty ass system is too old :^)"
                    );
                }

                let lhs = self.ensure_register(*lhs);
                let lhs = self.xmm(lhs);

                let rhs = self.ensure_register(*rhs);
                let rhs = self.xmm(rhs);

                let dst = self.def(dst);
                let dst = self.xmm(dst);

                self.emit(match op {
                    BinOp::DAdd => Insn::VAddSd { dst, lhs, rhs },
                    BinOp::DSub => Insn::VSubSd { dst, lhs, rhs },
                    BinOp::DMul => Insn::VMulSd { dst, lhs, rhs },
                    _ => Insn::VDivSd { dst, lhs, rhs },
                });
            }
            ir::Instr::Bin {
                op: op @ (BinOp::DLt | BinOp::DGt),
                dst,
                lhs,
                rhs,
                ..
            } => {
                let lhs = self.ensure_register(*lhs);
                let lhs = self.xmm(lhs);

                let rhs = self.ensure_register(*rhs);
                let rhs = self.xmm(rhs);

                let dst = self.def(dst);
                let dst = self.gpr(dst);

                // ucomisd reports `<` like a NaN, via CF, but `>` only when
                // ordered, so `<` is lowered as the swapped `>`: false for NaN
                // like the VM.
                let (lhs, rhs) = if *op == BinOp::DLt {
                    (rhs, lhs)
                } else {
                    (lhs, rhs)
                };
                self.emit(Insn::Ucomisd { lhs, rhs });
                self.emit(Insn::MovImm { dst, imm: 0 });
                self.emit(Insn::Seta { dst });
            }
            ir::Instr::Bin {
                op, dst, lhs, rhs, ..
            } => {
                let divisor = *rhs;
                let lhs = self.ensure_register(*lhs);
                let lhs = self.gpr(lhs);
                let rhs = self.ensure_register(*rhs);
                let rhs = self.gpr(rhs);
                match op {
                    BinOp::IAdd
                    | BinOp::ISub
                    | BinOp::IMul
                    | BinOp::IAnd
                    | BinOp::IOr
                    | BinOp::IXor => {
                        let dst = self.def(dst);
                        let dst = self.gpr(dst);

                        // x86 arithmetic is `dst <op>= src`, so dst has to
                        // start out holding lhs.
                        if dst == lhs {
                            self.emit(arith(*op, dst, rhs));
                        } else if dst != rhs {
                            self.emit(Insn::Mov { dst, src: lhs });
                            self.emit(arith(*op, dst, rhs));
                        } else if *op == BinOp::ISub {
                            // dst holds rhs: rhs - lhs negated is lhs - rhs.
                            self.emit(Insn::Sub { dst, src: lhs });
                            self.emit(Insn::Neg { reg: dst });
                        } else {
                            // add, mul, and, or and xor commute.
                            self.emit(arith(*op, dst, lhs));
                        }
                    }
                    // the bmi2 forms take the count from any register, mod 64 like the ir op
                    BinOp::IShl | BinOp::IShr | BinOp::IUShr => {
                        if !std::arch::is_x86_feature_detected!("bmi2") {
                            bail!(self, "{op:?} needs bmi2");
                            return;
                        }
                        let dst = self.def(dst);
                        let dst = self.gpr(dst);
                        let (src, count) = (lhs, rhs);
                        self.emit(match op {
                            BinOp::IShl => Insn::Shlx { dst, src, count },
                            BinOp::IShr => Insn::Sarx { dst, src, count },
                            _ => Insn::Shrx { dst, src, count },
                        });
                    }
                    // Strings are interned: equal contents share one pointer.
                    BinOp::IEq | BinOp::BEq | BinOp::SEq | BinOp::ILt | BinOp::IGt => {
                        let dst = self.def(dst);
                        let dst = self.gpr(dst);
                        self.emit(Insn::Cmp { lhs, rhs });
                        self.emit(Insn::MovImm { dst, imm: 0 });
                        self.emit(match op {
                            BinOp::ILt => Insn::Setl { dst },
                            BinOp::IGt => Insn::Setg { dst },
                            _ => Insn::Sete { dst },
                        });
                    }
                    BinOp::IDiv | BinOp::IMod => {
                        // A register divisor can't be checked at compile time.
                        if self.func.nonzero(self.block, divisor) {
                            purple_garden_shared::trace!(
                                "[jit::x86] {}: %v{} is nonzero in b{}, dropped its zero check",
                                self.func.name,
                                divisor.0,
                                self.block.0
                            );
                        } else {
                            self.emit(Insn::Test { lhs: rhs, rhs });
                            let nonzero = encode::jump(self.out, Cond::NotZero);
                            self.trap_div_zero();
                            let resume = self.out.len();
                            patch_rel32(self.out, nonzero, nonzero + 4, resume)
                                .expect("short forward jump");
                        }
                        self.div(*op, dst.id, lhs, Divisor::Reg(rhs));
                    }
                    _ => bail!(self, "unsupported bin op {op:?}"),
                }
            }
            ir::Instr::Cast { dst, from, .. } => {
                use ir::ptype::Type::{Bool, Double, Int};
                let src = self.ensure_register(from.id);
                let to = self.def(dst);
                match (&from.ty, &dst.ty) {
                    (Int, Double) => {
                        let src = self.gpr(src);
                        let dst = self.xmm(to);
                        self.emit(Insn::Zeroxmm { dst });
                        self.emit(Insn::Cvtsi2sd { dst, src });
                    }
                    // Like Rust's `as`: cvttsd2si gives i64::MIN for NaN and both overflows, the
                    // conditional moves fix up NaN to 0 and positive overflow to i64::MAX.
                    (Double, Int) => {
                        let src = self.xmm(src);
                        let dst = self.gpr(to);
                        self.emit(Insn::Cvttsd2si { dst, src });
                        self.emit(Insn::MovAbs {
                            dst: GPR_SCRATCH,
                            imm: 2f64.powi(63).to_bits(),
                        });
                        self.emit(Insn::MovqToXmm {
                            dst: XMM_SCRATCH,
                            src: GPR_SCRATCH,
                        });
                        self.emit(Insn::Ucomisd {
                            lhs: src,
                            rhs: XMM_SCRATCH,
                        });
                        self.emit(Insn::MovAbs {
                            dst: GPR_SCRATCH,
                            imm: i64::MAX as u64,
                        });
                        self.emit(Insn::Cmovae {
                            dst,
                            src: GPR_SCRATCH,
                        });
                        self.emit(Insn::Ucomisd { lhs: src, rhs: src });
                        // mov leaves the flags alone, xor wouldn't
                        self.emit(Insn::MovImm {
                            dst: GPR_SCRATCH,
                            imm: 0,
                        });
                        self.emit(Insn::Cmovp {
                            dst,
                            src: GPR_SCRATCH,
                        });
                    }
                    (Int, Bool) => {
                        let src = self.gpr(src);
                        let dst = self.gpr(to);
                        self.emit(Insn::Test { lhs: src, rhs: src });
                        self.emit(Insn::MovImm { dst, imm: 0 });
                        self.emit(Insn::Setne { dst });
                    }
                    // Bool and Int share the same representation.
                    (Bool, Int) => {
                        let src = self.gpr(src);
                        let dst = self.gpr(to);
                        if dst != src {
                            self.emit(Insn::Mov { dst, src });
                        }
                    }
                    _ => bail!(self, "unsupported cast from {} to {}", from.ty, dst.ty),
                }
            }
            ir::Instr::Lookup {
                dst,
                subject,
                entries,
                default,
                ..
            } => {
                let func = self.func;
                let entries = func.entries(*entries);
                // Slots as for a switch, holding values instead of targets.
                let step = match entries[0].0 {
                    ir::Const::Str(_) => string::ALIGN as i64,
                    _ => 1,
                };
                let mut values = Vec::with_capacity_in(entries.len() * 2 + 1, self.scratch());
                for c in entries.iter().flat_map(|(k, v)| [k, v]).chain([default]) {
                    let value = match c {
                        ir::Const::Str(_) => {
                            let Some(&value) = self
                                .globals
                                .get(c)
                                .and_then(|&slot| self.strings.get(slot as usize))
                            else {
                                bail!(self, "lookup string missing from the const pool");
                                return;
                            };
                            value
                        }
                        c => Value::from(c.clone()),
                    };
                    values.push(value.0);
                }
                let default = values.pop().unwrap();
                let mut pairs = Vec::with_capacity_in(values.len() / 2, self.scratch());
                pairs.extend(values.chunks(2).map(|kv| (kv[0] as i64, kv[1])));
                let first = pairs.iter().map(|&(key, _)| key).min().unwrap();
                let slot = |key: i64| ((key - first) / step) as usize;
                let len = pairs.iter().map(|&(key, _)| slot(key)).max().unwrap() + 1;
                let Ok(len_imm) = i32::try_from(len) else {
                    bail!(self, "lookup table of {len} slots");
                    return;
                };

                let subject = self.ensure_register(*subject);
                let subject = self.gpr(subject);
                let dst = self.def(dst);
                let dst = self.gpr(dst);

                // rax = the subject's slot, everything outside the table wraps
                // to a huge slot and takes the default.
                self.emit(Insn::MovAbs {
                    dst: GPR_SCRATCH,
                    imm: (first as u64).wrapping_neg(),
                });
                self.emit(Insn::Add {
                    dst: GPR_SCRATCH,
                    src: subject,
                });
                if step > 1 {
                    self.emit(Insn::ShrImm {
                        dst: GPR_SCRATCH,
                        imm: step.trailing_zeros() as u8,
                    });
                }
                self.emit(Insn::CmpImm {
                    reg: GPR_SCRATCH,
                    imm: len_imm,
                });
                let to_default = encode::jump(self.out, Cond::AboveEq);

                // dst holds the table address until it holds the value.
                self.emit(Insn::LeaRip { dst, disp: 0 });
                let lea = self.out.len() - 4;
                self.emit(Insn::LoadScaled8 {
                    dst,
                    base: dst,
                    index: GPR_SCRATCH,
                });
                let to_done = encode::jump(self.out, Cond::Always);

                while !self.out.len().is_multiple_of(8) {
                    self.out.push(0xcc);
                }
                let table = self.out.len();
                patch_rel32(self.out, lea, lea + 4, table).expect("table follows the lea");
                let mut slots = Vec::with_capacity_in(len, self.scratch());
                slots.resize(len, default);
                for &(key, value) in &pairs {
                    slots[slot(key)] = value;
                }
                for value in slots {
                    self.out.extend_from_slice(&value.to_le_bytes());
                }
                self.tables.push(table..self.out.len());

                let default_at = self.out.len();
                patch_rel32(self.out, to_default, to_default + 4, default_at)
                    .expect("default follows the table");
                self.emit(Insn::MovAbs { dst, imm: default });
                let done = self.out.len();
                patch_rel32(self.out, to_done, to_done + 4, done)
                    .expect("lookup ends after the default");
            }
            ir::Instr::Sys { dst, fun, args, .. } => {
                let func = self.func;
                let args = func.params(*args);
                let Some(native) = fun.native else {
                    bail!(self, "{} has no native entry", fun.name);
                    return;
                };
                let Some(moves) = self.native_entry_moves(fun, args) else {
                    return;
                };
                self.call_native_entry(native, moves, fun, dst);
            }
            ir::Instr::Call {
                dst,
                func: callee,
                args,
                ..
            } => {
                let callee = if *callee == self.func.id {
                    Callee::This
                } else if let Some(&f) = self.natives.get(callee) {
                    Callee::Native(f)
                } else {
                    bail!(self, "f{} is not compiled natively", callee.0);
                    return;
                };
                let func = self.func;
                self.call_native(callee, func.params(*args), dst);
            }
            ir::Instr::Un { op, dst, src, .. } => {
                let feature = match op {
                    UnOp::Assert | UnOp::INot | UnOp::DSqrt => true,
                    UnOp::IPopcnt => std::arch::is_x86_feature_detected!("popcnt"),
                    UnOp::ICtz => std::arch::is_x86_feature_detected!("bmi1"),
                    UnOp::IClz => std::arch::is_x86_feature_detected!("lzcnt"),
                };
                if !feature {
                    bail!(self, "{op:?} is missing its cpu feature");
                    return;
                }
                let src = self.ensure_register(*src);
                let insn = match op {
                    UnOp::Assert => {
                        let cond = self.gpr(src);
                        self.emit(Insn::Test {
                            lhs: cond,
                            rhs: cond,
                        });
                        let holds = encode::jump(self.out, Cond::NotZero);
                        self.call(
                            purple_garden_runtime::jit_trap_assert as *const () as u64,
                            &[AbiArg::Reg(VM)],
                            None,
                        );
                        let resume = self.out.len();
                        patch_rel32(self.out, holds, holds + 4, resume)
                            .expect("short forward jump");
                        // a Void value is never read, we dont reserve a reg
                        return;
                    }
                    UnOp::INot => {
                        let dst = self.def(dst);
                        let (dst, src) = (self.gpr(dst), self.gpr(src));
                        if dst != src {
                            self.emit(Insn::Mov { dst, src });
                        }
                        Insn::Not { reg: dst }
                    }
                    UnOp::IPopcnt => {
                        let dst = self.def(dst);
                        Insn::Popcnt {
                            dst: self.gpr(dst),
                            src: self.gpr(src),
                        }
                    }
                    UnOp::ICtz => {
                        let dst = self.def(dst);
                        Insn::Tzcnt {
                            dst: self.gpr(dst),
                            src: self.gpr(src),
                        }
                    }
                    UnOp::IClz => {
                        let dst = self.def(dst);
                        Insn::Lzcnt {
                            dst: self.gpr(dst),
                            src: self.gpr(src),
                        }
                    }
                    UnOp::DSqrt => {
                        let dst = self.def(dst);
                        Insn::VSqrtSd {
                            dst: self.xmm(dst),
                            src: self.xmm(src),
                        }
                    }
                };
                self.emit(insn);
            }
            _ => bail!(self, "unsupported instruction {i:?}"),
        }
    }

    fn term(&mut self, t: Option<&ir::Terminator>, next: Option<ir::Id>) {
        let Some(term) = t else {
            return;
        };
        let func = self.func;

        match term {
            ir::Terminator::Return { value, .. } => {
                // an Assert's Void value has no register, see its lowering
                if let Some(value) = value
                    .as_ref()
                    .filter(|&&value| self.ra.location(value).is_some())
                {
                    match self.ensure_register(*value) {
                        // only enter_native calls the entry, it reads the result from rax
                        Preg::Fpr(fpr) if self.func.id == ir::Id(0) => {
                            self.emit(Insn::MovqFromXmm {
                                dst: RAX,
                                src: Xmm(fpr),
                            });
                        }
                        Preg::Fpr(fpr) if fpr != 0 => self.mov(Preg::Fpr(0), Preg::Fpr(fpr)),
                        Preg::Gpr(gpr) if gpr != RAX.0 => {
                            self.mov(Preg::Gpr(RAX.0), Preg::Gpr(gpr))
                        }
                        _ => {}
                    }
                }
                self.jump(Cond::Always, Target::Epilogue);
            }
            ir::Terminator::Jump { id, params, .. } => {
                let src = func.params(*params);
                let dst = func.params(func.blocks[id.0 as usize].params);
                self.edge_moves(src, dst);
                if Some(*id) != next {
                    self.jump(Cond::Always, Target::Block(*id));
                }
            }
            ir::Terminator::Branch {
                cond,
                yes: (yes, yes_params),
                no: (no, no_params),
                ..
            } => {
                let yes_src = func.params(*yes_params);
                let yes_dst = func.params(func.blocks[yes.0 as usize].params);
                let no_src = func.params(*no_params);
                let no_dst = func.params(func.blocks[no.0 as usize].params);

                let cond = self.ensure_register(*cond);
                let cond = self.gpr(cond);
                self.emit(Insn::Test {
                    lhs: cond,
                    rhs: cond,
                });
                self.branch(
                    Cond::NotZero,
                    (*yes, yes_src, yes_dst),
                    (*no, no_src, no_dst),
                    next,
                );
            }
            ir::Terminator::BranchCmpImm {
                op: BinOp::IEq,
                lhs,
                imm,
                yes: (yes, yes_params),
                no: (no, no_params),
                ..
            } => {
                let yes_src = func.params(*yes_params);
                let yes_dst = func.params(func.blocks[yes.0 as usize].params);
                let no_src = func.params(*no_params);
                let no_dst = func.params(func.blocks[no.0 as usize].params);

                let lhs = self.ensure_register(*lhs);
                let lhs = self.gpr(lhs);
                if *imm == 0 {
                    self.emit(Insn::Test { lhs, rhs: lhs });
                } else {
                    self.emit(Insn::CmpImm {
                        reg: lhs,
                        imm: *imm,
                    });
                }
                self.branch(
                    Cond::Zero,
                    (*yes, yes_src, yes_dst),
                    (*no, no_src, no_dst),
                    next,
                );
            }
            ir::Terminator::BranchCmp {
                op: BinOp::SEq,
                lhs,
                rhs,
                yes: (yes, yes_params),
                no: (no, no_params),
                ..
            } => {
                let yes_src = func.params(*yes_params);
                let yes_dst = func.params(func.blocks[yes.0 as usize].params);
                let no_src = func.params(*no_params);
                let no_dst = func.params(func.blocks[no.0 as usize].params);

                let lhs = self.ensure_register(*lhs);
                let lhs = self.gpr(lhs);
                let rhs = self.ensure_register(*rhs);
                let rhs = self.gpr(rhs);
                // Strings are interned: equal contents share one pointer.
                self.emit(Insn::Cmp { lhs, rhs });
                self.branch(
                    Cond::Zero,
                    (*yes, yes_src, yes_dst),
                    (*no, no_src, no_dst),
                    next,
                );
            }
            ir::Terminator::Tail {
                func: callee, args, ..
            } if *callee == func.id => {
                let entry_params = func.params(func.blocks[self.entry.0 as usize].params);
                self.edge_moves(func.params(*args), entry_params);
                self.jump(Cond::Always, Target::Block(self.entry));
            }
            ir::Terminator::Tail {
                func: callee, args, ..
            } => {
                let Some(&f) = self.natives.get(callee) else {
                    bail!(self, "f{} is not compiled natively", callee.0);
                    return;
                };
                let args = func.params(*args);
                if args.len() > ARG_GPRS.len() {
                    bail!(self, "{} args, more than argument registers", args.len());
                    return;
                }
                // nothing outlives a tail call, the arguments are all that is left to move
                let mut moves = std::mem::replace(self.move_pairs, Vec::new_in(self.scratch()));
                moves.clear();
                for (i, &arg) in args.iter().enumerate() {
                    let src = self.location(arg);
                    let dst = Location::Reg(arg_reg(self.classes[arg.0 as usize], i));
                    if src != dst {
                        moves.push((src, dst));
                    }
                }
                self.parallel_move(moves);
                self.jump(Cond::Always, Target::Tail(f as usize));
            }
            ir::Terminator::Switch {
                subject,
                cases,
                default,
                ..
            } => {
                // The table of bc's Op::Switch: a slot per value from the lowest
                // key. Strings are interned, so a string key is its address in
                // the string pool, where strings sit ALIGN bytes apart.
                let cases = func.cases(*cases);
                let step = match cases[0].key {
                    ir::Const::Str(_) => string::ALIGN as i64,
                    _ => 1,
                };
                let mut keys = Vec::with_capacity(cases.len());
                for case in cases {
                    let key = match &case.key {
                        ir::Const::Int(v) => *v,
                        key => {
                            let Some(&value) = self
                                .globals
                                .get(key)
                                .and_then(|&slot| self.strings.get(slot as usize))
                            else {
                                bail!(self, "switch key missing from the const pool");
                                return;
                            };
                            value.0 as i64
                        }
                    };
                    let dst = func.params(func.blocks[case.target.0.0 as usize].params);
                    let moves = self.edge_has_moves(func.params(case.target.1), dst);
                    keys.push((key, case.target, moves));
                }
                let first = keys.iter().map(|&(key, ..)| key).min().unwrap();
                let slot = |key: i64| ((key - first) / step) as usize;
                let len = keys.iter().map(|&(key, ..)| slot(key)).max().unwrap() + 1;
                let Ok(len_imm) = i32::try_from(len) else {
                    bail!(self, "switch table of {len} slots");
                    return;
                };

                // An edge that moves goes through a pad after the table, the table's holes take
                // the default's.
                let default_dst = func.params(func.blocks[default.0.0 as usize].params);
                let default_moves = self.edge_has_moves(func.params(default.1), default_dst);
                let mut pads = Vec::new_in(self.scratch());
                let mut entry = |edge: (ir::Id, ir::ParamsId), moves: bool| {
                    if moves {
                        pads.push(edge);
                        Err(pads.len() - 1)
                    } else {
                        Ok(edge.0)
                    }
                };
                let default_entry = entry(*default, default_moves);
                let mut targets = Vec::with_capacity_in(len, self.scratch());
                targets.resize(len, default_entry);
                for &(key, edge, moves) in &keys {
                    targets[slot(key)] = entry(edge, moves);
                }

                let subject = self.ensure_register(*subject);
                let subject = self.gpr(subject);
                // the table base reuses the subject's register once rax is computed from it, kept
                // on the stack meanwhile if a case still reads it, through a param or its moves
                let base = subject;
                let keep_base = keys.iter().any(|&(_, (target, _), moves)| {
                    moves
                        || func
                            .params(func.blocks[target.0 as usize].params)
                            .iter()
                            .any(|&param| {
                                let (start, end) = self.liveness[param.0 as usize];
                                end > start
                                    && self.ra.location(param)
                                        == Some(Location::Reg(Preg::Gpr(base.0)))
                            })
                });

                // rax = the subject's slot. A subject below `first` wraps around
                // to a huge slot, so one unsigned compare sends everything
                // outside the table to the default.
                self.emit(Insn::MovAbs {
                    dst: GPR_SCRATCH,
                    imm: (first as u64).wrapping_neg(),
                });
                self.emit(Insn::Add {
                    dst: GPR_SCRATCH,
                    src: subject,
                });
                if step > 1 {
                    self.emit(Insn::ShrImm {
                        dst: GPR_SCRATCH,
                        imm: step.trailing_zeros() as u8,
                    });
                }
                self.emit(Insn::CmpImm {
                    reg: GPR_SCRATCH,
                    imm: len_imm,
                });
                let to_default_pad = match default_entry {
                    Ok(_) => {
                        self.jump(Cond::AboveEq, Target::Block(default.0));
                        None
                    }
                    Err(_) => Some(encode::jump(self.out, Cond::AboveEq)),
                };

                // Jump through the table placed right after the jmp. Its entries
                // are offsets from the table start, so the code stays relocatable.
                if keep_base {
                    self.emit(Insn::Push { reg: base });
                }
                self.emit(Insn::LeaRip { dst: base, disp: 0 });
                let lea = self.out.len() - 4;
                self.emit(Insn::LoadI32Scaled {
                    dst: GPR_SCRATCH,
                    base,
                    index: GPR_SCRATCH,
                });
                self.emit(Insn::Add {
                    dst: GPR_SCRATCH,
                    src: base,
                });
                if keep_base {
                    self.emit(Insn::Pop { reg: base });
                }
                self.emit(Insn::JmpReg { reg: GPR_SCRATCH });

                while !self.out.len().is_multiple_of(4) {
                    self.out.push(0xcc);
                }
                let table = self.out.len();
                patch_rel32(self.out, lea, lea + 4, table).expect("table follows the lea");
                let mut pad_entries = Vec::new_in(self.scratch());
                for target in targets {
                    match target {
                        Ok(block) => self.patches.push(Patch {
                            rel: self.out.len(),
                            target: Target::Block(block),
                            from: table,
                        }),
                        Err(pad) => pad_entries.push((self.out.len(), pad)),
                    }
                    self.out.extend_from_slice(&[0; 4]);
                }
                self.tables.push(table..self.out.len());

                let mut pad_at = Vec::with_capacity_in(pads.len(), self.scratch());
                for (target, args) in pads {
                    pad_at.push(self.out.len());
                    let dst = func.params(func.blocks[target.0 as usize].params);
                    self.edge_moves(func.params(args), dst);
                    self.jump(Cond::Always, Target::Block(target));
                }
                for (rel, pad) in pad_entries {
                    let offset = (pad_at[pad] - table) as i32;
                    self.out[rel..rel + 4].copy_from_slice(&offset.to_le_bytes());
                }
                if let (Some(rel), Err(pad)) = (to_default_pad, default_entry)
                    && patch_rel32(self.out, rel, rel + 4, pad_at[pad]).is_none()
                {
                    bail!(self, "switch default pad out of rel32 range");
                }
            }
            _ => bail!(self, "unsupported terminator"),
        }
    }

    /// Jump to `yes` on `cond`, else move the no edge and go to `no`. Whichever
    /// target is laid out `next` is reached by falling through instead.
    ///
    /// The moves of one edge may overwrite what the other edge reads, so each edge moves only on
    /// its own path. Moves leave the flags alone.
    fn branch(
        &mut self,
        cond: Cond,
        (yes, yes_src, yes_dst): (ir::Id, &[ir::Id], &[ir::Id]),
        (no, no_src, no_dst): (ir::Id, &[ir::Id], &[ir::Id]),
        next: Option<ir::Id>,
    ) {
        if self.edge_has_moves(yes_src, yes_dst) {
            // the edge going to `next` goes last and falls through
            let (first, last, to_last) = if Some(yes) == next {
                ((no, no_src, no_dst), (yes, yes_src, yes_dst), cond)
            } else {
                ((yes, yes_src, yes_dst), (no, no_src, no_dst), cond.invert())
            };
            let to_last_at = encode::jump(self.out, to_last);
            self.edge_moves(first.1, first.2);
            self.jump(Cond::Always, Target::Block(first.0));
            let last_at = self.out.len();
            if patch_rel32(self.out, to_last_at, to_last_at + 4, last_at).is_none() {
                bail!(self, "edge moves out of rel32 range");
            }
            self.edge_moves(last.1, last.2);
            if Some(last.0) != next {
                self.jump(Cond::Always, Target::Block(last.0));
            }
            return;
        }
        self.jump(cond, Target::Block(yes));
        let moves_at = self.out.len();
        self.edge_moves(no_src, no_dst);
        if Some(no) == next {
            return;
        }
        // The no edge moved nothing, so the jcc is still the last instruction
        // and can be flipped to target `no`, falling through to `yes`.
        if Some(yes) == next && self.out.len() == moves_at {
            self.patches.pop();
            self.out.truncate(moves_at - 6);
            self.jump(cond.invert(), Target::Block(no));
            return;
        }
        self.jump(Cond::Always, Target::Block(no));
    }

    fn edge_has_moves(&self, src: &[ir::Id], dst: &[ir::Id]) -> bool {
        src.iter().zip(dst).any(|(&s, &d)| {
            let (start, end) = self.liveness[d.0 as usize];
            end > start && self.ra.location(s) != self.ra.location(d)
        })
    }

    /// Move edge args `src` into the target's block params `dst`, the IR's phi
    /// nodes, as if every move happened at once.
    fn edge_moves(&mut self, src: &[ir::Id], dst: &[ir::Id]) {
        debug_assert_eq!(src.len(), dst.len(), "edge arity matches target params");
        let mut todo = std::mem::replace(self.move_pairs, Vec::new_in(self.scratch()));
        todo.clear();
        for (&s, &d) in src.iter().zip(dst) {
            // nothing reads it, it may share its register with a sibling
            let (start, end) = self.liveness[d.0 as usize];
            if end <= start {
                continue;
            }
            let (s, d) = (self.location(s), self.location(d));
            if s != d {
                todo.push((s, d));
            }
        }
        self.parallel_move(todo);
    }

    /// Performs the `(src, dst)` moves as if all at once: a move runs once no pending move still
    /// reads its dst. When only cycles remain, one src is parked in the scratch register of its
    /// class and its readers read that instead, which turns its cycle into a chain. A slot is
    /// parked in the gpr scratch, it holds a word of either class.
    fn parallel_move(&mut self, mut todo: Vec<(Location, Location), S>) {
        // A save that found no register after a bail is `(reg, reg)`, it would block forever
        todo.retain(|&(src, dst)| src != dst);
        while !todo.is_empty() {
            match todo
                .iter()
                .position(|&(_, dst)| todo.iter().all(|&(src, _)| src != dst))
            {
                Some(i) => {
                    let (src, dst) = todo.swap_remove(i);
                    self.move_location(dst, src);
                }
                None => {
                    let parked = todo[0].0;
                    let scratch = match parked {
                        Location::Reg(Preg::Fpr(_)) => Preg::Fpr(XMM_SCRATCH.0),
                        _ => Preg::Gpr(GPR_SCRATCH.0),
                    };
                    let scratch = Location::Reg(scratch);
                    self.move_location(scratch, parked);
                    for (src, _) in &mut todo {
                        if *src == parked {
                            *src = scratch;
                        }
                    }
                }
            }
        }
        *self.move_pairs = todo;
    }

    /// One move of [`Lowering::parallel_move`], slot to slot goes through the first gpr temp: a
    /// function with slots never allocates it.
    fn move_location(&mut self, dst: Location, src: Location) {
        match (dst, src) {
            (Location::Reg(dst), Location::Reg(src)) => self.mov(dst, src),
            (Location::Reg(dst), Location::Stack(slot)) => self.load_slot(dst, slot),
            (Location::Stack(slot), Location::Reg(src)) => self.store_slot(slot, src),
            (Location::Stack(dst), Location::Stack(src)) => {
                let tmp = temp(Class::Gpr, 0);
                self.load_slot(tmp, src);
                self.store_slot(dst, tmp);
            }
        }
    }

    fn mov(&mut self, dst: Preg, src: Preg) {
        match (dst, src) {
            (Preg::Gpr(dst), Preg::Gpr(src)) => self.emit(Insn::Mov {
                dst: Reg(dst),
                src: Reg(src),
            }),
            (Preg::Fpr(dst), Preg::Fpr(src)) => self.emit(Insn::Movaps {
                dst: Xmm(dst),
                src: Xmm(src),
            }),
            (Preg::Gpr(dst), Preg::Fpr(src)) => self.emit(Insn::MovqFromXmm {
                dst: Reg(dst),
                src: Xmm(src),
            }),
            (Preg::Fpr(dst), Preg::Gpr(src)) => self.emit(Insn::MovqToXmm {
                dst: Xmm(dst),
                src: Reg(src),
            }),
        }
    }

    /// Copy the values living in registers this instruction clobbers into the free registers
    /// [`Lowering::plan`] picked; [`Lowering::restore`] copies them back.
    fn save_clobbered(&mut self) -> Vec<(Preg, Location), S> {
        let saves = self.planned_copies();
        for &(reg, copy) in &saves {
            self.move_location(copy, Location::Reg(reg));
        }
        saves
    }

    /// The most preferred free register of `class` that `clobbers` leaves alone.
    fn take_unclobbered(&mut self, class: Class, clobbers: Clobbers) -> Option<Preg> {
        let pool = match class {
            Class::Gpr => GPR_POOL,
            Class::Fpr => XMM_POOL,
        };
        pool.iter()
            .rev()
            .map(|&num| Preg::new(class, num))
            .filter(|&reg| !clobbers.contains(reg))
            .find_map(|reg| self.ra.try_take(reg))
    }

    /// Plan a `(reg, copy)` pair for every value living in `clobbers` past `pos`, taking the copies
    /// until [`Lowering::give_back_copies`]. Without a free register the copy is a slot.
    fn reserve_copies(&mut self, clobbers: Clobbers, pos: u32) {
        let start = self.copies.len();
        self.copies.extend(
            self.ra
                .live_after(pos)
                .filter(|&reg| clobbers.contains(reg))
                .map(|reg| (reg, Location::Reg(reg))),
        );
        for i in start..self.copies.len() {
            // A call clobbers every fpr, so a live double moves into a callee saved gpr instead.
            let reg = self.copies[i].0;
            match self
                .take_unclobbered(reg.class(), clobbers)
                .or_else(|| self.take_unclobbered(Class::Gpr, clobbers))
            {
                Some(free) => self.copies[i].1 = Location::Reg(free),
                None => self.copies[i].1 = Location::Stack(self.ra.take_slot()),
            }
            purple_garden_shared::trace!(
                "[jit::x86] row {pos}: save {reg} in {}",
                self.copies[i].1
            );
        }
    }

    /// The `(reg, copy)` pairs planned for the current row.
    fn planned_copies(&mut self) -> Vec<(Preg, Location), S> {
        let mut saves = std::mem::replace(self.saves, Vec::new_in(self.scratch()));
        saves.clear();
        let row = self.pos as usize;
        let (start, end) = (
            self.copies_at[row] as usize,
            self.copies_at[row + 1] as usize,
        );
        saves.extend_from_slice(&self.copies[start..end]);
        saves
    }

    /// Moves the arguments of a call into place and copies the values live across it out of the
    /// registers it clobbers, as one parallel move: the registers of arguments dying at the call
    /// can hold the copies, the resolver reads them first.
    fn move_args_and_save(
        &mut self,
        mut moves: Vec<(Location, Location), S>,
    ) -> Vec<(Preg, Location), S> {
        let saves = self.planned_copies();
        moves.extend(saves.iter().map(|&(reg, copy)| (Location::Reg(reg), copy)));
        self.parallel_move(moves);
        saves
    }

    fn restore(&mut self, saves: Vec<(Preg, Location), S>) {
        for &(reg, copy) in &saves {
            self.move_location(Location::Reg(reg), copy);
        }
        *self.saves = saves;
    }

    /// `idiv` divides `rdx:rax`, so the dividend goes through `rax` and `cqo`
    /// clobbers `rdx`. A divisor without a usable register (an immediate, or
    /// one living in `rdx`) is staged in `rcx`.
    ///
    /// `i64::MIN % -1` faults in `idiv` (#DE); the bytecode VM panics on the
    /// same input via Rust's checked `%`, so neither path is lenient here.
    fn div(&mut self, op: BinOp, dst: ir::Id, lhs: Reg, divisor: Divisor) {
        let saves = self.save_clobbered();
        let dst = self.def_id(dst);
        let dst = self.gpr(dst);

        self.emit(Insn::Mov { dst: RAX, src: lhs });
        let divisor = match divisor {
            Divisor::Reg(r) if r != RDX => r,
            Divisor::Reg(r) => {
                self.emit(Insn::Mov { dst: RCX, src: r });
                RCX
            }
            Divisor::Imm(imm) => {
                self.emit(Insn::MovImm { dst: RCX, imm });
                RCX
            }
        };
        // 64 bit idiv costs ~1.7x of 32 bit div on some cores, take the latter when both operands
        // are in [0, 2^32): signed and unsigned agree there, and anything negative has its high
        // bits set. rdx is free, cqo clobbers it anyway, and on the jump it is the zero high half
        // div expects. The narrow path is the single taken jump, like LLVM lays it out.
        self.emit(Insn::Mov { dst: RDX, src: RAX });
        self.emit(Insn::Or {
            dst: RDX,
            src: divisor,
        });
        self.emit(Insn::ShrImm { dst: RDX, imm: 32 });
        let narrow = encode::jump(self.out, Cond::Zero);
        self.emit(Insn::Cqo);
        self.emit(Insn::Idiv { divisor });
        let done = encode::jump(self.out, Cond::Always);
        let fast = self.out.len();
        patch_rel32(self.out, narrow, narrow + 4, fast).expect("short forward jump");
        self.emit(Insn::Div32 { divisor });
        let end = self.out.len();
        patch_rel32(self.out, done, done + 4, end).expect("short forward jump");
        let src = if op == BinOp::IDiv { RAX } else { RDX };
        self.emit(Insn::Mov { dst, src });

        self.restore(saves);
    }

    /// Call the native function `callee`: argument i in `ARG_GPRS[i]` or `xmm{i}` by its class, the
    /// result in `rax` or `xmm0`, no memory in between. Natives keep `rbx` and the callee saved
    /// registers, so only values live across the call in the others are copied away. More
    /// arguments than registers would need the stack and stay bytecode.
    fn call_native(&mut self, callee: Callee, args: &[ir::Id], dst: &ir::TypeId<'ir>) {
        if args.len() > ARG_GPRS.len() {
            bail!(self, "{} args, more than argument registers", args.len());
            return;
        }
        let mut moves = std::mem::replace(self.move_pairs, Vec::new_in(self.scratch()));
        moves.clear();
        for (i, &arg) in args.iter().enumerate() {
            let src = self.location(arg);
            let dst = Location::Reg(arg_reg(self.classes[arg.0 as usize], i));
            if src != dst {
                moves.push((src, dst));
            }
        }
        let saves = self.move_args_and_save(moves);
        self.calls = true;
        match callee {
            Callee::Native(f) => {
                self.emit(Insn::CallRel { disp: 0 });
                self.native_refs.push((self.out.len() - 4, f as usize));
            }
            Callee::This => {
                self.emit(Insn::CallRel { disp: 0 });
                self.self_refs.push(self.out.len() - 4);
            }
        }
        let ret = match Class::from(&dst.ty) {
            Class::Gpr => Preg::Gpr(RAX.0),
            Class::Fpr => Preg::Fpr(0),
        };
        self.take_result(dst, ret, saves);
    }

    /// Restores `saves` and moves a call's result from `ret` into `dst`. `dst` may take the
    /// register of a value dying right after the call, so the restores go first; a double restored
    /// into xmm0 would overwrite a result there, it waits in the scratch register then.
    fn take_result(
        &mut self,
        dst: &ir::TypeId<'ir>,
        mut ret: Preg,
        saves: Vec<(Preg, Location), S>,
    ) {
        if saves.iter().any(|&(reg, _)| reg == ret) {
            self.mov(Preg::Fpr(XMM_SCRATCH.0), ret);
            ret = Preg::Fpr(XMM_SCRATCH.0);
        }
        self.restore(saves);
        let void = dst.ty == ir::ptype::Type::Void;
        let dst = self.def(dst);
        // a Void value is never read, whatever the register holds will do
        if !void && dst != ret {
            self.mov(dst, ret);
        }
    }

    /// The moves placing `args` where SysV wants them for `fun`'s native entry, after the vm in
    /// `rdi`: Doubles in xmm registers, everything else in gprs. A slot crosses as a `Value` word,
    /// so a Double bound to it moves over into a gpr. More arguments than registers would need the
    /// stack and bail.
    fn native_entry_moves(
        &mut self,
        fun: &ir::Fn<'_>,
        args: &[ir::Id],
    ) -> Option<Vec<(Location, Location), S>> {
        let mut moves = std::mem::replace(self.move_pairs, Vec::new_in(self.scratch()));
        moves.clear();
        let (mut gprs, mut xmms) = (NATIVE_ENTRY_GPRS.iter(), 0..NATIVE_ENTRY_XMMS);
        for (&arg, ty) in args.iter().zip(fun.args) {
            let class = self.classes[arg.0 as usize];
            let src = self.location(arg);
            let dst = match Class::from(ty) {
                Class::Gpr => gprs.next().map(|r| Preg::Gpr(r.0)),
                Class::Fpr => xmms.next().map(Preg::Fpr),
            };
            let Some(dst) = dst else {
                bail!(self, "{} args, more than argument registers", args.len());
                return None;
            };
            assert!(
                class == dst.class() || matches!(ty, ir::ptype::Type::Slot(_)),
                "typecheck lets a {class:?} through as {ty:?}"
            );
            let dst = Location::Reg(dst);
            if src != dst {
                moves.push((src, dst));
            }
        }
        Some(moves)
    }

    /// Calls a builtin's native entry: arguments and result in registers, without `vm.r`. The vm
    /// goes into `rdi` last, an argument may still have been there.
    fn call_native_entry(
        &mut self,
        native: ir::NativeFn,
        moves: Vec<(Location, Location), S>,
        fun: &ir::Fn<'_>,
        dst: &ir::TypeId<'ir>,
    ) {
        let saves = self.move_args_and_save(moves);
        self.calls = true;
        self.emit(Insn::Mov { dst: RDI, src: VM });
        self.emit(Insn::MovAbs {
            dst: GPR_SCRATCH,
            imm: native as usize as u64,
        });
        self.emit(Insn::CallReg { reg: GPR_SCRATCH });
        // a slot result is a Value word, also when bound to a Double
        let ret = match Class::from(&fun.ret) {
            Class::Gpr => Preg::Gpr(RAX.0),
            Class::Fpr => Preg::Fpr(0),
        };
        self.take_result(dst, ret, saves);
    }

    /// Raise the trap, the helper leaves native code through `jit_abort`:
    /// nothing live needs to survive the call.
    fn trap_div_zero(&mut self) {
        self.call(
            purple_garden_runtime::jit_trap_div_zero as *const () as u64,
            &[AbiArg::Reg(VM)],
            None,
        );
    }

    /// SysV call to the helper at `addr`. The VM base lives in callee saved `rbx`, so it survives
    /// the call. Arguments are moved in order, which is only safe because callers pass the VM base
    /// and immediates.
    fn call(&mut self, addr: u64, args: &[AbiArg], result: Option<Reg>) {
        const ARGS: [Reg; 6] = [RDI, RSI, RDX, RCX, R8, R9];
        debug_assert!(args.len() <= ARGS.len());
        self.calls = true;
        for (&dst, arg) in ARGS.iter().zip(args) {
            match *arg {
                AbiArg::Reg(src) if src != dst => self.emit(Insn::Mov { dst, src }),
                AbiArg::Reg(_) => {}
                AbiArg::Imm(imm) => self.emit(Insn::MovAbs { dst, imm }),
            }
        }
        self.emit(Insn::MovAbs {
            dst: GPR_SCRATCH,
            imm: addr,
        });
        self.emit(Insn::CallReg { reg: GPR_SCRATCH });
        if let Some(dst) = result.filter(|&dst| dst != RAX) {
            self.emit(Insn::Mov { dst, src: RAX });
        }
    }

    fn jump(&mut self, cond: Cond, target: Target) {
        let rel = encode::jump(self.out, cond);
        self.patches.push(Patch {
            rel,
            target,
            from: rel + 4,
        });
    }

    /// Resolve every jump. The body ends where the epilogue is appended, so a
    /// return jump ending it is dropped and falls through instead.
    fn patch_jumps(&mut self) {
        if let Some(last) = self.patches.last()
            && last.target == Target::Epilogue
            && last.rel + 4 == self.out.len()
        {
            self.patches.pop();
            self.out.truncate(self.out.len() - 5);
        }
        let end = self.out.len();
        for i in 0..self.patches.len() {
            let Patch { rel, target, from } = self.patches[i];
            let offset = match target {
                Target::Tail(f) => {
                    self.tail_refs.push((rel, f));
                    continue;
                }
                Target::Epilogue => end,
                // A block that started at the dropped jump now starts at the epilogue.
                Target::Block(id) => self.block_offsets[id.0 as usize].min(end),
            };
            if patch_rel32(self.out, rel, from, offset).is_none() {
                bail!(self, "jump out of rel32 range");
            }
        }
    }
}
