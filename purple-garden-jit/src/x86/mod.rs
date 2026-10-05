//! x86-64 JIT backend
//!
//! IR is lowered to native code in a single pass, uses xralloc2 to map SSA values to x86 GPRs while emitting.
//!
//! The native ABI passes `*mut Vm` in `rdi`. `Vm::r` being the first field, `rdi` is the base of the VM
//! register file. Arguments arrive in `vm.r[0..n]`; return register is `vm.r[0]`. All
//! computation happen in GPRs. The jit moves arguments from `vm.r[0..n]` to GPRs in a jitted
//! functions prologue.

mod encode;

use std::collections::HashMap;

use crate::regalloc::Xralloc2;
use encode::{
    Cond, Insn, R8, R9, R10, R11, R12, R13, R14, R15, RAX, RBX, RCX, RDI, RDX, RSI, RSP, Reg,
    patch_rel32,
};
use purple_garden_ir::{self as ir, BinOp};
use purple_garden_runtime::Value;

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
const SCRATCH: Reg = RAX;

/// Allocatable GPRs, popped from the back: caller-saved first so leaf functions dont require a
/// prologue, `rcx`/`rdx` last since `idiv` clobbers them.
const POOL: &[u8] = &[
    RBX.0, R12.0, R13.0, R14.0, R15.0, RDX.0, RCX.0, R11.0, R10.0, R9.0, R8.0, RSI.0,
];
const CALLEE_SAVED: &[Reg] = &[RBX, R12, R13, R14, R15];
/// Allocatable registers a SysV helper call clobbers.
const CALLER_SAVED: &[Reg] = &[RCX, RDX, RSI, R8, R9, R10, R11];
/// Stack a helper call reserves for the VM base; keeps rsp 16-byte aligned
/// since the prologue aligns it for functions with calls.
const CALL_STACK: i32 = 16;

/// Compile one IR function into x86-64 machine code, returning `None` if unsupported constructs are
/// included
pub fn compile_func<'ir>(
    func: &ir::Func<'ir>,
    out: &mut Vec<u8>,
    liveness: &[(u32, u32)],
    globals: &HashMap<ir::Const<'ir>, u32>,
    strings: &[Value],
    ra: &mut Xralloc2,
    buffers: &mut Scratch,
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
        // required boundary state, nothing to do here other than return
        Insn::Ret.encode(out);
        purple_garden_shared::trace!("[jit::x86] compiled {} ({} bytes)", func.name, out.len());
        return Some(());
    }

    ra.reset(liveness.len(), POOL);
    let calls = Lowering::new(func, liveness, globals, strings, ra, buffers, entry).lower()?;

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
    let saved = CALLEE_SAVED.iter().copied().filter(|reg| ra.used(reg.0));

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
    for reg in saved.clone() {
        Insn::Push { reg }.encode(out);
    }
    if pad {
        Insn::SubImm { dst: RSP, imm: 8 }.encode(out);
    }
    out.extend_from_slice(&buffers.body);
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

fn is_result_slot_identity(func: &ir::Func<'_>, entry: ir::Id) -> bool {
    let Some(&result_param) = func.params.first() else {
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
}

/// Reusable lowering allocation storage
#[derive(Debug, Default, Clone)]
pub struct Scratch {
    body: Vec<u8>,
    block_offsets: Vec<usize>,
    patches: Vec<Patch>,
    move_pairs: Vec<(Reg, Reg)>,
    /// `(reg, copy)` pairs preserved across a clobbering instruction.
    saves: Vec<(Reg, Reg)>,
}

/// Argument of a SysV helper call.
#[derive(Clone, Copy)]
enum AbiArg {
    Reg(Reg),
    Imm(u64),
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
struct Lowering<'a, 'ir> {
    func: &'a ir::Func<'ir>,
    liveness: &'a [(u32, u32)],
    /// `vm.globals` slot of each constant.
    globals: &'a HashMap<ir::Const<'ir>, u32>,
    /// `strings[slot]` is the final address of the string constant in `slot`.
    strings: &'a [Value],
    ra: &'a mut Xralloc2,
    out: &'a mut Vec<u8>,
    entry: ir::Id,
    /// Walks in lockstep with [`ir::Func::live_set_into`]: two units per block
    /// header, instruction and terminator, uses on `pos`, defs on `pos + 1`.
    pos: u32,
    /// Set once a helper is called; the prologue then aligns the stack.
    calls: bool,
    unsupported: bool,
    /// `block_offsets[block_id]` is the body offset of the block's first byte,
    /// `usize::MAX` for blocks not emitted.
    block_offsets: &'a mut Vec<usize>,
    patches: &'a mut Vec<Patch>,
    /// Reusable `(src, dst)` buffer for the parallel-move resolver in
    /// [`Lowering::edge_moves`].
    move_pairs: &'a mut Vec<(Reg, Reg)>,
    /// Reusable `(reg, copy)` buffer of [`Lowering::save_clobbered`].
    saves: &'a mut Vec<(Reg, Reg)>,
}

impl<'a, 'ir> Lowering<'a, 'ir> {
    fn new(
        func: &'a ir::Func<'ir>,
        liveness: &'a [(u32, u32)],
        globals: &'a HashMap<ir::Const<'ir>, u32>,
        strings: &'a [Value],
        ra: &'a mut Xralloc2,
        buffers: &'a mut Scratch,
        entry: ir::Id,
    ) -> Self {
        let Scratch {
            body,
            block_offsets,
            patches,
            move_pairs,
            saves,
        } = buffers;
        body.clear();
        block_offsets.clear();
        block_offsets.resize(func.blocks.len(), usize::MAX);
        patches.clear();
        Self {
            func,
            liveness,
            globals,
            strings,
            ra,
            out: body,
            entry,
            pos: 0,
            calls: false,
            unsupported: false,
            block_offsets,
            patches,
            move_pairs,
            saves,
        }
    }

    /// Emit the body, ending where the epilogue is appended. Returns whether
    /// it calls helpers, `None` if the function can't be compiled.
    fn lower(mut self) -> Option<bool> {
        // Args arrive in the VM register file: param i in vm.r[i].
        for (slot, &param) in self.func.params.iter().enumerate() {
            let used = self
                .liveness
                .get(param.0 as usize)
                .is_some_and(|&(start, _)| start != u32::MAX);
            if used {
                let dst = self.def(param, 0);
                self.emit(Insn::LoadSlot {
                    dst,
                    slot: slot as u8,
                });
            }
        }

        for block in &self.func.blocks {
            if block.tombstone {
                continue;
            }
            self.block_offsets[block.id.0 as usize] = self.out.len();
            for &param in self.func.params(block.params) {
                self.def(param, self.pos);
            }
            self.pos += 2;

            for instruction in &block.instructions {
                self.instr(instruction);
                if self.unsupported {
                    return None;
                }
                self.pos += 2;
            }

            self.term(block.term.as_ref());
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

    /// Register of `id`, assigned if this is its first touch. `at` is where it
    /// is first needed; a forward edge needs a block param before its block
    /// starts. Registers are only reclaimed up to the interval start, so the
    /// register is clear of every value the interval overlaps.
    fn def(&mut self, id: ir::Id, at: u32) -> Reg {
        let (start, last_use) = self.liveness[id.0 as usize];
        match self.ra.alloc(id, at.min(start), last_use) {
            Some(reg) => Reg(reg),
            None => {
                bail!(self, "out of registers at %v{}", id.0);
                SCRATCH
            }
        }
    }

    fn ensure_register(&mut self, id: ir::Id) -> Reg {
        match self.ra.reg(id) {
            Some(reg) => Reg(reg),
            // Not proven impossible: the block layout isn't guaranteed to put
            // every definition before its uses.
            None => {
                bail!(self, "use of %v{} before its definition", id.0);
                SCRATCH
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
                let base = self.ensure_register(*base);
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
                let dst = self.def(dst.id, self.pos + 1);
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
                let dst = self.def(dst.id, self.pos + 1);
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
                let saves = self.save_clobbered(CALLER_SAVED);
                let dst = self.def(*id, self.pos + 1);
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
                let dst = self.def(dst.id, self.pos + 1);
                self.emit(Insn::MovAbs { dst, imm: value.0 });
            }
            ir::Instr::LoadConst { dst, value, .. } => {
                let Some(imm) = (match value {
                    ir::Const::False => Some(0),
                    ir::Const::True => Some(1),
                    ir::Const::Int(i) => i32::try_from(*i).ok(),
                    _ => None,
                }) else {
                    bail!(self, "const is not a bool or i32");
                    return;
                };
                let dst = self.def(dst.id, self.pos + 1);
                self.emit(Insn::MovImm { dst, imm });
            }
            ir::Instr::BinImm {
                op, dst, lhs, imm, ..
            } => {
                let lhs = self.ensure_register(*lhs);
                let imm = *imm;
                match op {
                    BinOp::IAdd | BinOp::ISub => {
                        let dst = self.def(dst.id, self.pos + 1);
                        if dst != lhs {
                            self.emit(Insn::Mov { dst, src: lhs });
                        }
                        self.emit(match op {
                            BinOp::IAdd => Insn::AddImm { dst, imm },
                            _ => Insn::SubImm { dst, imm },
                        });
                    }
                    BinOp::IEq => {
                        let dst = self.def(dst.id, self.pos + 1);
                        if imm == 0 {
                            self.emit(Insn::Test { lhs, rhs: lhs });
                        } else {
                            self.emit(Insn::CmpImm { reg: lhs, imm });
                        }
                        // mov leaves the flags alone; sete only writes the low byte.
                        self.emit(Insn::MovImm { dst, imm: 0 });
                        self.emit(Insn::Sete { dst });
                    }
                    BinOp::IDiv | BinOp::IMod if imm == 0 => {
                        self.def(dst.id, self.pos + 1);
                        self.trap_div_zero();
                    }
                    BinOp::IMod if imm == 2 => {
                        let dst = self.def(dst.id, self.pos + 1);
                        if dst != lhs {
                            self.emit(Insn::Mov { dst, src: lhs });
                        }
                        self.emit(Insn::AndImm { dst, imm: 1 });
                    }
                    BinOp::IDiv | BinOp::IMod => self.div(*op, dst.id, lhs, Divisor::Imm(imm)),
                    _ => bail!(self, "unsupported binimm op {op:?}"),
                }
            }
            ir::Instr::Bin {
                op, dst, lhs, rhs, ..
            } => {
                let lhs = self.ensure_register(*lhs);
                let rhs = self.ensure_register(*rhs);
                match op {
                    BinOp::IAdd | BinOp::ISub | BinOp::IMul => {
                        let dst = self.def(dst.id, self.pos + 1);
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
                    BinOp::IEq | BinOp::SEq => {
                        let dst = self.def(dst.id, self.pos + 1);
                        self.emit(Insn::Cmp { lhs, rhs });
                        self.emit(Insn::MovImm { dst, imm: 0 });
                        self.emit(Insn::Sete { dst });
                    }
                    BinOp::IDiv | BinOp::IMod => {
                        // A register divisor can't be checked at compile time.
                        self.emit(Insn::Test { lhs: rhs, rhs });
                        let nonzero = encode::jump(self.out, Cond::NotZero);
                        self.trap_div_zero();
                        let resume = self.out.len();
                        patch_rel32(self.out, nonzero, resume).expect("short forward jump");
                        self.div(*op, dst.id, lhs, Divisor::Reg(rhs));
                    }
                    _ => bail!(self, "unsupported bin op {op:?}"),
                }
            }
            _ => bail!(self, "unsupported instruction {i:?}"),
        }
    }

    fn term(&mut self, t: Option<&ir::Terminator>) {
        let Some(term) = t else {
            return;
        };
        let func = self.func;
        let pos = self.pos;

        match term {
            ir::Terminator::Return { value, .. } => {
                if let Some(value) = value {
                    let src = self.ensure_register(*value);
                    self.emit(Insn::StoreSlot { src, slot: 0 });
                }
                self.jump(Cond::Always, Target::Epilogue);
            }
            ir::Terminator::Jump { id, params, .. } => {
                let src = func.params(*params);
                let dst = func.params(func.blocks[id.0 as usize].params);
                self.edge_moves(src, dst, pos);
                self.jump(Cond::Always, Target::Block(*id));
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
                self.emit(Insn::Test {
                    lhs: cond,
                    rhs: cond,
                });
                self.jump(Cond::NotZero, Target::Block(*yes));
                self.edge_moves(no_src, no_dst, pos + 1);
                self.jump(Cond::Always, Target::Block(*no));
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
                if *imm == 0 {
                    self.emit(Insn::Test { lhs, rhs: lhs });
                } else {
                    self.emit(Insn::CmpImm {
                        reg: lhs,
                        imm: *imm,
                    });
                }
                self.jump(Cond::Zero, Target::Block(*yes));
                self.edge_moves(no_src, no_dst, pos + 1);
                self.jump(Cond::Always, Target::Block(*no));
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
                let rhs = self.ensure_register(*rhs);
                // Strings are interned: equal contents share one pointer.
                self.emit(Insn::Cmp { lhs, rhs });
                self.jump(Cond::Zero, Target::Block(*yes));
                self.edge_moves(no_src, no_dst, pos + 1);
                self.jump(Cond::Always, Target::Block(*no));
            }
            ir::Terminator::Tail {
                func: callee, args, ..
            } if *callee == func.id => {
                self.edge_moves(args, &func.params, pos);
                self.jump(Cond::Always, Target::Block(self.entry));
            }
            _ => bail!(self, "unsupported terminator"),
        }
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
        let mut todo = std::mem::take(self.move_pairs);
        todo.clear();
        for (&s, &d) in src.iter().zip(dst) {
            let s = self.ensure_register(s);
            let d = self.def(d, at);
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
                    self.emit(Insn::Mov { dst, src });
                    todo.swap_remove(i);
                    continue 'outer;
                }
            }

            let (start_src, start_dst) = todo.swap_remove(0);
            self.emit(Insn::Mov {
                dst: SCRATCH,
                src: start_src,
            });
            let mut cur_freed = start_src;
            while let Some(idx) = todo.iter().position(|&(_, d)| d == cur_freed) {
                let (src, dst) = todo.swap_remove(idx);
                self.emit(Insn::Mov { dst, src });
                cur_freed = src;
            }
            self.emit(Insn::Mov {
                dst: start_dst,
                src: SCRATCH,
            });
        }

        *self.move_pairs = todo;
    }

    /// Copy values living in `clobbers` past this instruction into free
    /// registers; [`Lowering::restore`] copies them back. Runs before the
    /// instruction's dst is assigned, so neither a copy nor dst can take an
    /// operand register the instruction still reads.
    fn save_clobbered(&mut self, clobbers: &[Reg]) -> Vec<(Reg, Reg)> {
        let mut saves = std::mem::take(self.saves);
        saves.clear();
        saves.extend(
            self.ra
                .live_after(self.pos)
                .map(Reg)
                .filter(|reg| clobbers.contains(reg))
                .map(|reg| (reg, reg)),
        );
        for save in &mut saves {
            match self.ra.take_free(|free| !clobbers.contains(&Reg(free))) {
                Some(free) => save.1 = Reg(free),
                None => bail!(
                    self,
                    "no free register to preserve {} across a clobber",
                    save.0
                ),
            }
        }
        for &(reg, copy) in &saves {
            self.emit(Insn::Mov {
                dst: copy,
                src: reg,
            });
        }
        saves
    }

    fn restore(&mut self, saves: Vec<(Reg, Reg)>) {
        for &(reg, copy) in &saves {
            self.emit(Insn::Mov {
                dst: reg,
                src: copy,
            });
            self.ra.give_back(copy.0);
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
        let clobbers: &[Reg] = if staged { &[RCX, RDX] } else { &[RDX] };
        let saves = self.save_clobbered(clobbers);
        let dst = self.def(dst, self.pos + 1);

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

    /// Raise the trap and leave: nothing live needs to survive the call.
    fn trap_div_zero(&mut self) {
        let helper: purple_garden_runtime::BuiltinFn = purple_garden_runtime::jit_trap_div_zero;
        self.call(helper as usize as u64, &[AbiArg::Reg(VM)], None);
        self.jump(Cond::Always, Target::Epilogue);
    }

    /// SysV call to `addr`. The VM base lives in caller-saved `rdi`, so it is
    /// kept on the stack across the call. Arguments are moved in order, which
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
            dst: SCRATCH,
            imm: addr,
        });
        self.emit(Insn::CallReg { reg: SCRATCH });
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
        self.patches.push(Patch { rel, target });
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
            let Patch { rel, target } = self.patches[i];
            let offset = match target {
                Target::Epilogue => end,
                // A block that started at the dropped jump now starts at the epilogue.
                Target::Block(id) => self.block_offsets[id.0 as usize].min(end),
            };
            if patch_rel32(self.out, rel, offset).is_none() {
                bail!(self, "jump out of rel32 range");
            }
        }
    }
}
