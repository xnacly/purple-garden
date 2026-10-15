//! x86-64 JIT backend
//!
//! IR is lowered to native code in a single pass, uses xralloc2 to map SSA values to x86 GPRs while emitting.
//!
//! The native ABI passes `*mut Vm` in `rdi`. `Vm::r` being the first field, `rdi` is the base of the VM
//! register file. Arguments arrive in `vm.r[0..n]`; return register is `vm.r[0]`. All
//! computation happen in GPRs. The jit moves arguments from `vm.r[0..n]` to GPRs in a jitted
//! functions prologue.
//!
//! On return `rax` holds the trap flag: zero, or nonzero once a trap is pending
//! and every native frame must unwind to the interpreter. Native functions
//! preserve `rdi` and clobber only the SysV caller-saved registers, so native
//! to native calls are plain `call`s: no trampoline, no spill of the VM base.
//! The interpreter calls natives as a [`BuiltinFn`] and ignores `rax`.

mod encode;

use std::{alloc::Allocator, collections::HashMap};

use crate::{
    arch::encode::Xmm,
    xralloc2::{
        Class::{self, Fpr},
        Reg as Preg, Xralloc2,
    },
};
use encode::{
    Cond, Insn, R8, R9, R10, R11, R12, R13, R14, R15, RAX, RBX, RCX, RDI, RDX, RSI, RSP, Reg,
    patch_rel32,
};
use purple_garden_ir::{self as ir, BinOp};
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

/// `rdi` holds `*mut Vm` == `&vm.r[0]`, base for slot loads/stores.
const VM: Reg = RDI;
/// Never holds a value: breaks edge-move cycles and holds helper addresses.
const GPR_SCRATCH: Reg = RAX;

/// Allocatable GPRs, popped from the back: caller-saved first so leaf functions dont require a
/// prologue, `rcx`/`rdx` last since `idiv` clobbers them.
const GPR_POOL: &[u8] = &[
    RBX.0, R12.0, R13.0, R14.0, R15.0, RDX.0, RCX.0, R11.0, R10.0, R9.0, R8.0, RSI.0,
];
const GPR_CALLEE_SAVED: &[Reg] = &[RBX, R12, R13, R14, R15];
/// Allocatable registers a SysV helper call clobbers.
const GPR_CALLER_SAVED: &[Reg] = &[RCX, RDX, RSI, R8, R9, R10, R11];
/// A helper call clobbers every xmm on top: SysV has no callee-saved fpr.
const CALL_CLOBBERS: Clobbers = Clobbers {
    gprs: GPR_CALLER_SAVED,
    fprs: true,
};
/// Stack a helper call reserves for the VM base; keeps rsp 16-byte aligned
/// since the prologue aligns it for functions with calls.
const CALL_STACK: i32 = 16;

/// Allocatable XMM regs, also popped from the back; none are callee saved
const XMM_POOL: &[u8] = &[14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0];
/// scratch reg, not in pool, cycles and helper loc
const XMM_SCRATCH: Xmm = Xmm(15);

/// Compile one IR function into x86-64 machine code, returning `None` if unsupported constructs are
/// included
pub fn compile_func<'ir, F: Allocator, S: Allocator + Clone, N: Allocator>(
    func: &ir::Func<'ir, F>,
    out: &mut Vec<u8, S>,
    liveness: &[(u32, u32)],
    globals: &HashMap<ir::Const<'ir>, u32>,
    strings: &[Value],
    natives: &crate::Natives<N>,
    ra: &mut Xralloc2<S>,
    buffers: &mut Scratch<S>,
) -> Option<()> {
    if func.params.len() > 32 {
        skip!(
            func,
            "too many params for disp8 slot loads: {}",
            func.params.len()
        );
    }

    let Some(entry) = func.blocks.iter().find(|b| !b.tombstone).map(|b| b.id) else {
        skip!(func, "empty function");
    };

    if is_result_slot_identity(func, entry) {
        // Native calls receive arg0 in vm.r[0] and must return through vm.r[0]
        //
        // When a function returns that parameter unchanged, the VM register file already holds the
        // required boundary state, nothing to do here other than return without a trap
        Insn::Zero { reg: GPR_SCRATCH }.encode(out);
        Insn::Ret.encode(out);
        purple_garden_shared::trace!("[jit::x86] compiled {} ({} bytes)", func.name, out.len());
        return Some(());
    }

    ra.reset(liveness.len(), GPR_POOL, XMM_POOL);
    let calls = Lowering::new(
        func, liveness, globals, strings, natives, ra, buffers, entry,
    )
    .lower()?;

    // The body is emitted first since the callee-saved registers it uses are
    // only known afterwards. Wrap it into a frame:
    //
    //     push <saved>     ; callee-saved regs the body used, the interpreter
    //                      ; calls us as a C function and expects them intact
    //     sub rsp, 8       ; pad, only if needed for alignment
    //     <body>           ; returns jump to the epilogue below
    //     add rsp, 8
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
    let pad = calls && saved.clone().count() % 2 == 0;

    if !pad && saved.clone().next().is_none() {
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
    if pad {
        Insn::SubImm { dst: RSP, imm: 8 }.encode(out);
    }
    let body_at = out.len();
    out.extend_from_slice(&buffers.body);
    // Self calls take the address of the function's first byte, in front of
    // the prologue that is only laid out now.
    for &rel in &buffers.self_refs {
        patch_rel32(out, body_at + rel, body_at + rel + 4, start).expect("within the function");
    }
    if pad {
        Insn::AddImm { dst: RSP, imm: 8 }.encode(out);
    }
    for reg in saved.rev() {
        Insn::Pop { reg }.encode(out);
    }
    Insn::Ret.encode(out);

    purple_garden_shared::trace!("[jit::x86] compiled {} ({} bytes)", func.name, out.len());
    Some(())
}

fn is_result_slot_identity<F: Allocator>(func: &ir::Func<'_, F>, entry: ir::Id) -> bool {
    let Some(result_param) = func.params.first().map(|p| p.id) else {
        return false;
    };
    let block = &func.blocks[entry.0 as usize];
    block.instructions.is_empty()
        && matches!(
            block.term,
            Some(ir::Terminator::Return {
                value: Some(value),
                ..
            }) if value == result_param
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Block(ir::Id),
    /// The shared epilogue appended after the body.
    Epilogue,
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
    move_pairs: Vec<(Preg, Preg), S>,
    /// `(reg, copy)` pairs preserved across a clobbering instruction.
    saves: Vec<(Preg, Preg), S>,
    /// Body offsets of the rel32s (self calls) that address the function's own start.
    self_refs: Vec<usize, S>,
}

impl<S: Allocator + Clone> Scratch<S> {
    pub fn new_in(scratch: S) -> Self {
        Self {
            body: Vec::new_in(scratch.clone()),
            block_offsets: Vec::new_in(scratch.clone()),
            patches: Vec::new_in(scratch.clone()),
            move_pairs: Vec::new_in(scratch.clone()),
            saves: Vec::new_in(scratch.clone()),
            self_refs: Vec::new_in(scratch),
        }
    }
}

/// Target of a call that passes its arguments through `vm.r`.
#[derive(Clone, Copy)]
enum Callee {
    /// A builtin: called through `jit_sys`, which reports whether it trapped.
    Sys(BuiltinFn),
    /// A natively compiled function: called directly, returns the trap flag.
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
        _ => unreachable!("only IAdd, ISub and IMul lower to two-operand arithmetic"),
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

/// Lowers one function, assigning registers as it emits.
///
/// Registers come from [`Xralloc2`] the first time a value is touched and stay
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
    /// Walks in lockstep with [`ir::Func::live_set_into`]: two units per block
    /// header, instruction and terminator, uses on `pos`, defs on `pos + 1`.
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
    move_pairs: &'a mut Vec<(Preg, Preg), S>,
    /// Reusable `(reg, copy)` buffer of [`Lowering::save_clobbered`].
    saves: &'a mut Vec<(Preg, Preg), S>,
    self_refs: &'a mut Vec<usize, S>,
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
            self_refs,
        } = buffers;
        body.clear();
        block_offsets.clear();
        block_offsets.resize(func.blocks.len(), usize::MAX);
        patches.clear();
        self_refs.clear();
        Self {
            func,
            liveness,
            globals,
            strings,
            natives,
            ra,
            out: body,
            entry,
            pos: 0,
            calls: false,
            unsupported: false,
            avx: std::arch::is_x86_feature_detected!("avx"),
            block_offsets,
            patches,
            move_pairs,
            saves,
            self_refs,
        }
    }

    fn scratch(&self) -> S {
        self.out.allocator().clone()
    }

    /// Emit the body, ending where the epilogue is appended. Returns whether
    /// it calls helpers, `None` if the function can't be compiled.
    fn lower(mut self) -> Option<bool> {
        // Native calls recurse on the machine stack: trap like the interpreter
        // before running off its end.
        let calls_native = self
            .func
            .blocks
            .iter()
            .filter(|b| !b.tombstone)
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i, ir::Instr::Call { .. }));
        if calls_native {
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
            self.jump(Cond::Always, Target::Epilogue);
            let resume = self.out.len();
            patch_rel32(self.out, fine, fine + 4, resume).expect("short forward jump");
        }

        // Args arrive in the VM register file: param i in vm.r[i].
        for (slot, param) in self.func.params.iter().enumerate() {
            let used = self
                .liveness
                .get(param.id.0 as usize)
                .is_some_and(|&(start, _)| start != u32::MAX);
            if used {
                let slot = slot as u8;
                match self.def(param, 0) {
                    Preg::Gpr(dst) => self.emit(Insn::LoadSlot {
                        dst: Reg(dst),
                        slot,
                    }),
                    Preg::Fpr(dst) => self.emit(Insn::LoadSlotSd {
                        dst: Xmm(dst),
                        slot,
                    }),
                }
            }
        }

        let blocks = &self.func.blocks;
        for (idx, block) in blocks.iter().enumerate() {
            if block.tombstone {
                continue;
            }
            self.block_offsets[block.id.0 as usize] = self.out.len();
            for &param in self.func.params(block.params) {
                self.def_class(param, self.pos, Class::Gpr);
            }
            self.pos += 2;

            for instruction in &block.instructions {
                self.instr(instruction);
                if self.unsupported {
                    return None;
                }
                self.pos += 2;
            }

            let next = blocks[idx + 1..]
                .iter()
                .find(|b| !b.tombstone)
                .map(|b| b.id);
            self.term(block.term.as_ref(), next);
            if self.unsupported {
                return None;
            }
            self.pos += 2;
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

    /// Register of `id`, assigned if this is its first touch. `at` is where it
    /// is first needed; a forward edge needs a block param before its block
    /// starts. Registers are only reclaimed up to the interval start, so the
    /// register is clear of every value the interval overlaps.
    fn def(&mut self, &ir::TypeId { id, ref ty }: &ir::TypeId<'ir>, at: u32) -> Preg {
        self.def_class(id, at, ty.into())
    }

    /// [`Lowering::def`] for the ids the IR doesn't type at the definition:
    /// params, whose class the caller picks, and block params, which take
    /// the class of the edge argument moved into them.
    fn def_class(&mut self, id: ir::Id, at: u32, class: Class) -> Preg {
        let (start, last_use) = self.liveness[id.0 as usize];
        match self.ra.alloc(id, class, at.min(start), last_use) {
            Some(reg) => reg,
            None => {
                bail!(self, "out of registers at %v{}", id.0);
                Preg::Gpr(GPR_SCRATCH.0)
            }
        }
    }

    fn ensure_register(&mut self, id: ir::Id) -> Preg {
        match self.ra.reg(id) {
            Some(reg) => reg,
            None => {
                bail!(self, "use of %v{} before its definition", id.0);
                Preg::Gpr(GPR_SCRATCH.0)
            }
        }
    }

    fn instr(&mut self, i: &ir::Instr<'ir>) {
        match i {
            ir::Instr::Noop => {}
            ir::Instr::Store {
                src, base, offset, ..
            } if *offset <= i32::MAX as u32 => {
                let src = self.ensure_register(*src);
                let src = self.gpr(src);
                let base = self.ensure_register(*base);
                let base = self.gpr(base);
                self.emit(Insn::StoreMem {
                    base,
                    offset: *offset,
                    src,
                });
            }
            ir::Instr::Load {
                dst, base, offset, ..
            } if *offset <= i32::MAX as u32 => {
                let base = self.ensure_register(*base);
                let base = self.gpr(base);
                let dst = self.def(dst, self.pos + 1);
                let dst = self.gpr(dst);
                self.emit(Insn::LoadMem {
                    dst,
                    base,
                    offset: *offset,
                });
            }
            ir::Instr::AddrOf {
                dst, base, offset, ..
            } if *offset <= i32::MAX as u32 => {
                let base = self.ensure_register(*base);
                let base = self.gpr(base);
                let dst = self.def(dst, self.pos + 1);
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
                let saves = self.save_clobbered(CALL_CLOBBERS);
                let dst = self.def_class(*id, self.pos + 1, ty.into());
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
                let dst = self.def(dst, self.pos + 1);
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
                let dst = self.def(dst, self.pos + 1);
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

                let dst = self.def(dst, self.pos + 1);
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
                let dst = self.def(dst, self.pos + 1);
                let dst = self.gpr(dst);

                match op {
                    BinOp::IAdd | BinOp::ISub => {
                        if dst != lhs {
                            self.emit(Insn::Mov { dst, src: lhs });
                        }
                        self.emit(match op {
                            BinOp::IAdd => Insn::AddImm { dst, imm },
                            _ => Insn::SubImm { dst, imm },
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

                let dst = self.def(dst, self.pos + 1);
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

                let dst = self.def(dst, self.pos + 1);
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
                let lhs = self.ensure_register(*lhs);
                let lhs = self.gpr(lhs);
                let rhs = self.ensure_register(*rhs);
                let rhs = self.gpr(rhs);
                match op {
                    BinOp::IAdd | BinOp::ISub | BinOp::IMul => {
                        let dst = self.def(dst, self.pos + 1);
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
                            // add and mul commute.
                            self.emit(arith(*op, dst, lhs));
                        }
                    }
                    // Strings are interned: equal contents share one pointer.
                    BinOp::IEq | BinOp::BEq | BinOp::SEq | BinOp::ILt | BinOp::IGt => {
                        let dst = self.def(dst, self.pos + 1);
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
                        self.emit(Insn::Test { lhs: rhs, rhs });
                        let nonzero = encode::jump(self.out, Cond::NotZero);
                        self.trap_div_zero();
                        let resume = self.out.len();
                        patch_rel32(self.out, nonzero, nonzero + 4, resume)
                            .expect("short forward jump");
                        self.div(*op, dst.id, lhs, Divisor::Reg(rhs));
                    }
                    _ => bail!(self, "unsupported bin op {op:?}"),
                }
            }
            ir::Instr::Cast { dst, from, .. }
                if from.ty == ir::ptype::Type::Int && dst.ty == ir::ptype::Type::Double =>
            {
                let src = self.ensure_register(from.id);
                let src = self.gpr(src);
                let dst = self.def(dst, self.pos + 1);
                let dst = self.xmm(dst);
                self.emit(Insn::Zeroxmm { dst });
                self.emit(Insn::Cvtsi2sd { dst, src });
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
                let dst = self.def(dst, self.pos + 1);
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
                self.call_builtin(Callee::Sys(fun.ptr), func.params(*args), dst);
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
                self.call_builtin(callee, func.params(*args), dst);
            }
            _ => bail!(self, "unsupported instruction {i:?}"),
        }
    }

    fn term(&mut self, t: Option<&ir::Terminator>, next: Option<ir::Id>) {
        let Some(term) = t else {
            return;
        };
        let func = self.func;
        let pos = self.pos;

        match term {
            ir::Terminator::Return { value, .. } => {
                if let Some(value) = value {
                    let src = self.ensure_register(*value);
                    match src {
                        Preg::Gpr(gpr) => {
                            self.emit(Insn::StoreSlot {
                                src: Reg(gpr),
                                slot: 0,
                            });
                        }
                        Preg::Fpr(fpr) => self.emit(Insn::StoreSlotSd {
                            src: Xmm(fpr),
                            slot: 0,
                        }),
                    }
                }

                self.emit(Insn::Zero { reg: GPR_SCRATCH });
                self.jump(Cond::Always, Target::Epilogue);
            }
            ir::Terminator::Jump { id, params, .. } => {
                let src = func.params(*params);
                let dst = func.params(func.blocks[id.0 as usize].params);
                self.edge_moves(src, dst, pos);
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

                // Same phasing as liveness: yes moves, then cond, then no moves.
                self.edge_moves(yes_src, yes_dst, pos);
                let cond = self.ensure_register(*cond);
                let cond = self.gpr(cond);
                self.emit(Insn::Test {
                    lhs: cond,
                    rhs: cond,
                });
                self.branch(Cond::NotZero, *yes, *no, no_src, no_dst, next);
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

                self.edge_moves(yes_src, yes_dst, pos);
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
                self.branch(Cond::Zero, *yes, *no, no_src, no_dst, next);
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

                self.edge_moves(yes_src, yes_dst, pos);
                let lhs = self.ensure_register(*lhs);
                let lhs = self.gpr(lhs);
                let rhs = self.ensure_register(*rhs);
                let rhs = self.gpr(rhs);
                // Strings are interned: equal contents share one pointer.
                self.emit(Insn::Cmp { lhs, rhs });
                self.branch(Cond::Zero, *yes, *no, no_src, no_dst, next);
            }
            ir::Terminator::Tail {
                func: callee, args, ..
            } if *callee == func.id => {
                let entry_params = func.params(func.blocks[self.entry.0 as usize].params);
                self.edge_moves(func.params(*args), entry_params, pos);
                self.jump(Cond::Always, Target::Block(self.entry));
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
                    keys.push((key, case.target.0));
                }
                let first = keys.iter().map(|&(key, _)| key).min().unwrap();
                let slot = |key: i64| ((key - first) / step) as usize;
                let len = keys.iter().map(|&(key, _)| slot(key)).max().unwrap() + 1;
                let Ok(len_imm) = i32::try_from(len) else {
                    bail!(self, "switch table of {len} slots");
                    return;
                };

                let subject = self.ensure_register(*subject);
                let subject = self.gpr(subject);
                let Some(base) = self.ra.take_free(Class::Gpr, |_| true) else {
                    bail!(self, "no free register for the switch table");
                    return;
                };
                let base = self.gpr(base);

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
                self.jump(Cond::AboveEq, Target::Block(default.0));

                // Jump through the table placed right after the jmp. Its entries
                // are offsets from the table start, so the code stays relocatable.
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
                self.emit(Insn::JmpReg { reg: GPR_SCRATCH });
                self.ra.give_back(Preg::Gpr(base.0));

                while !self.out.len().is_multiple_of(4) {
                    self.out.push(0xcc);
                }
                let table = self.out.len();
                patch_rel32(self.out, lea, lea + 4, table).expect("table follows the lea");
                let mut targets = Vec::with_capacity_in(len, self.scratch());
                targets.resize(len, default.0);
                for &(key, target) in &keys {
                    targets[slot(key)] = target;
                }
                for target in targets {
                    self.patches.push(Patch {
                        rel: self.out.len(),
                        target: Target::Block(target),
                        from: table,
                    });
                    self.out.extend_from_slice(&[0; 4]);
                }
            }
            _ => bail!(self, "unsupported terminator"),
        }
    }

    /// Jump to `yes` on `cond`, else move the no edge and go to `no`. Whichever
    /// target is laid out `next` is reached by falling through instead.
    fn branch(
        &mut self,
        cond: Cond,
        yes: ir::Id,
        no: ir::Id,
        no_src: &[ir::Id],
        no_dst: &[ir::Id],
        next: Option<ir::Id>,
    ) {
        self.jump(cond, Target::Block(yes));
        let moves_at = self.out.len();
        self.edge_moves(no_src, no_dst, self.pos + 1);
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

    /// Move edge args `src` into the target's block params `dst`, the IR's phi
    /// nodes, as if every move happened at once. `dst` registers are assigned
    /// at `at` if this edge is the first to reach them.
    ///
    /// Parallel-move: emit a direct Mov for any pending pair whose dst isn't
    /// another pending move's src. When only cycles remain (e.g. a->b, b->c,
    /// c->a), park one head in the scratch register and walk the cycle.
    fn edge_moves(&mut self, src: &[ir::Id], dst: &[ir::Id], at: u32) {
        debug_assert_eq!(src.len(), dst.len(), "edge arity matches target params");
        let mut todo = std::mem::replace(self.move_pairs, Vec::new_in(self.scratch()));
        todo.clear();
        for (&s, &d) in src.iter().zip(dst) {
            let s = self.ensure_register(s);
            let d = self.def_class(d, at, s.class());
            if s != d {
                todo.push((s, d));
            }
        }

        'outer: loop {
            if todo.is_empty() {
                break;
            }
            for i in 0..todo.len() {
                let (src, dst) = todo[i];
                if !todo.iter().any(|&(s, _)| s == dst) {
                    self.mov(dst, src);
                    todo.swap_remove(i);
                    continue 'outer;
                }
            }

            let (start_src, start_dst) = todo.swap_remove(0);
            let scratch = match start_src {
                Preg::Gpr(_) => Preg::Gpr(GPR_SCRATCH.0),
                Preg::Fpr(_) => Preg::Fpr(XMM_SCRATCH.0),
            };
            self.mov(scratch, start_src);
            let mut cur_freed = start_src;
            while let Some(idx) = todo.iter().position(|&(_, d)| d == cur_freed) {
                let (src, dst) = todo.swap_remove(idx);
                self.mov(dst, src);
                cur_freed = src;
            }
            self.mov(start_dst, scratch);
        }

        *self.move_pairs = todo;
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
            _ => bail!(self, "move between {src:?} and {dst:?}"),
        }
    }

    /// Copy values living in `clobbers` past this instruction into free
    /// registers; [`Lowering::restore`] copies them back. Runs before the
    /// instruction's dst is assigned, so neither a copy nor dst can take an
    /// operand register the instruction still reads.
    fn save_clobbered(&mut self, clobbers: Clobbers) -> Vec<(Preg, Preg), S> {
        let mut saves = std::mem::replace(self.saves, Vec::new_in(self.scratch()));
        saves.clear();
        saves.extend(
            self.ra
                .live_after(self.pos)
                .filter(|&reg| clobbers.contains(reg))
                .map(|reg| (reg, reg)),
        );
        for save in &mut saves {
            // A helper call clobbers every fpr, so a live double finds no
            // copy and the function falls back until fprs can be spilled.
            let class = save.0.class();
            match self
                .ra
                .take_free(class, |free| !clobbers.contains(Preg::new(class, free)))
            {
                Some(free) => save.1 = free,
                None => bail!(
                    self,
                    "no free register to preserve {:?} across a clobber",
                    save.0
                ),
            }
        }
        for &(reg, copy) in &saves {
            let dst = self.gpr(copy);
            let src = self.gpr(reg);
            self.emit(Insn::Mov { dst, src });
        }
        saves
    }

    fn restore(&mut self, saves: Vec<(Preg, Preg), S>) {
        for &(reg, copy) in &saves {
            let dst = self.gpr(reg);
            let src = self.gpr(copy);
            self.emit(Insn::Mov { dst, src });
            self.ra.give_back(copy);
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
        let staged = !matches!(divisor, Divisor::Reg(r) if r != RDX);
        let clobbers = Clobbers {
            gprs: if staged { &[RCX, RDX] } else { &[RDX] },
            fprs: false,
        };
        let saves = self.save_clobbered(clobbers);
        let dst = self.def_class(dst, self.pos + 1, Class::Gpr);
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
        self.emit(Insn::Cqo);
        self.emit(Insn::Idiv { divisor });
        let src = if op == BinOp::IDiv { RAX } else { RDX };
        self.emit(Insn::Mov { dst, src });

        self.restore(saves);
    }

    /// Call `callee` the way the interpreter calls a builtin: arguments in
    /// `vm.r[0..]`, the result in `vm.r[0]`, a trap leaves this function so
    /// the interpreter surfaces it. The callee may write its argument
    /// registers, but this function's caller only spilled those up to this
    /// function's own arity; only `entry`, which nothing calls with live
    /// registers, may pass more.
    ///
    /// Builtins go through `jit_sys` for the trap flag. Native functions return
    /// it themselves and keep `rdi`, so they are called directly: a trampoline
    /// in between doubles the `call`/`ret` pairs per frame, and deep recursion
    /// then mispredicts most returns once the return stack buffer wraps.
    fn call_builtin(&mut self, callee: Callee, args: &[ir::Id], dst: &ir::TypeId<'ir>) {
        if args.len() > self.func.params.len().max(1) && self.func.id != ir::Id(0) {
            bail!(
                self,
                "{} args would clobber registers the caller kept",
                args.len()
            );
            return;
        }
        if args.len() > 15 {
            bail!(self, "too many args for disp8 slot stores: {}", args.len());
            return;
        }
        for (slot, &arg) in args.iter().enumerate() {
            let src = self.ensure_register(arg);
            let src = self.gpr(src);
            self.emit(Insn::StoreSlot {
                src,
                slot: slot as u8,
            });
        }
        // The args are in their slots, so values they were the last use of free their registers
        // for the copies.
        self.ra.expire(self.pos);
        let saves = self.save_clobbered(CALL_CLOBBERS);
        match callee {
            Callee::Sys(f) => self.call(
                purple_garden_runtime::jit_sys as *const () as u64,
                &[AbiArg::Reg(VM), AbiArg::Imm(f as usize as u64)],
                None,
            ),
            Callee::Native(f) => {
                self.calls = true;
                self.emit(Insn::MovAbs {
                    dst: GPR_SCRATCH,
                    imm: f as usize as u64,
                });
                self.emit(Insn::CallReg { reg: GPR_SCRATCH });
            }
            Callee::This => {
                self.calls = true;
                self.emit(Insn::CallRel { disp: 0 });
                self.self_refs.push(self.out.len() - 4);
            }
        }
        self.emit(Insn::Test { lhs: RAX, rhs: RAX });
        self.jump(Cond::NotZero, Target::Epilogue);
        self.restore(saves);
        let dst = self.def(dst, self.pos + 1);
        let dst = self.gpr(dst);
        self.emit(Insn::LoadSlot { dst, slot: 0 });
    }

    /// Raise the trap and leave with the flag the helper returns in `rax`:
    /// nothing live needs to survive the call.
    fn trap_div_zero(&mut self) {
        self.call(
            purple_garden_runtime::jit_trap_div_zero as *const () as u64,
            &[AbiArg::Reg(VM)],
            None,
        );
        self.jump(Cond::Always, Target::Epilogue);
    }

    /// SysV call to the helper at `addr`. The VM base lives in caller-saved
    /// `rdi`, so it is kept on the stack across the call. Arguments are moved in order, which
    /// is only safe because callers pass the VM base and immediates.
    fn call(&mut self, addr: u64, args: &[AbiArg], result: Option<Reg>) {
        const ARGS: [Reg; 6] = [RDI, RSI, RDX, RCX, R8, R9];
        debug_assert!(args.len() <= ARGS.len());
        self.calls = true;
        self.emit(Insn::SubImm {
            dst: RSP,
            imm: CALL_STACK,
        });
        self.emit(Insn::StoreMem {
            base: RSP,
            offset: 0,
            src: VM,
        });
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
        self.emit(Insn::LoadMem {
            dst: VM,
            base: RSP,
            offset: 0,
        });
        self.emit(Insn::AddImm {
            dst: RSP,
            imm: CALL_STACK,
        });
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
