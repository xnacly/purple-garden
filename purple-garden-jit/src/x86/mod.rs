//! x86-64 JIT backend
//!
//! IR is lowered to native code in a single pass, uses xralloc2 to map SSA values to x86 GPRs while emitting.
//!
//! The native ABI passes `*mut Vm` in `rdi`. `Vm::r` being the first field, `rdi` is the base of the VM
//! register file. Arguments arrive in `vm.r[0..n]`; return register is `vm.r[0]`. All
//! computation happen in GPRs. The jit moves arguments from `vm.r[0..n]` to GPRs in a jitted
//! functions prologue.

mod encode;

use crate::regalloc::Xralloc2;
use encode::{
    Cond, Insn, R8, R9, R10, R11, R12, R13, R14, R15, RAX, RBX, RCX, RDI, RDX, RSI, RSP, Reg,
    patch_rel32,
};
use purple_garden_ir::{self as ir, BinOp};

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
pub fn compile_func(
    func: &ir::Func<'_>,
    out: &mut Vec<u8>,
    liveness: &[(u32, u32)],
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
    let calls = Lowering::new(func, liveness, ra, buffers, entry).emit()?;

    // Pushes realign rsp from 8 (mod 16) at entry; helper calls need it at 0.
    let saved = CALLEE_SAVED.iter().copied().filter(|reg| ra.used(reg.0));
    let pad = calls && saved.clone().count() % 2 == 0;
    if !pad && saved.clone().next().is_none() {
        // The epilogue is a bare `ret`: return in place instead of jumping to it.
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

fn supported_const(value: &ir::Const<'_>) -> Option<i32> {
    match value {
        ir::Const::False => Some(0),
        ir::Const::True => Some(1),
        ir::Const::Int(i) => i32::try_from(*i).ok(),
        _ => None,
    }
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

/// Lowers one function, assigning registers as it emits.
///
/// Registers come from [`Xralloc2`] the first time a value is touched and stay
/// fixed for the value's whole liveness interval, so every CFG edge agrees on
/// where a value lives. `pos` walks in lockstep with
/// [`ir::Func::live_set_into`]: two units per block header, instruction and
/// terminator, uses on `pos`, defs on `pos + 1`.
///
/// Instructions clobbering fixed registers go through [`Lowering::clobbering`],
/// which copies the values living in those registers out and back.
struct Lowering<'a, 'ir> {
    func: &'a ir::Func<'ir>,
    liveness: &'a [(u32, u32)],
    ra: &'a mut Xralloc2,
    out: &'a mut Vec<u8>,
    entry: ir::Id,
    pos: u32,
    calls: bool,
    block_offsets: &'a mut Vec<usize>,
    patches: &'a mut Vec<Patch>,
    move_pairs: &'a mut Vec<(Reg, Reg)>,
    saves: &'a mut Vec<(Reg, Reg)>,
}

impl<'a, 'ir> Lowering<'a, 'ir> {
    fn new(
        func: &'a ir::Func<'ir>,
        liveness: &'a [(u32, u32)],
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
            ra,
            out: body,
            entry,
            pos: 0,
            calls: false,
            block_offsets,
            patches,
            move_pairs,
            saves,
        }
    }

    /// Emit the body, ending where the epilogue is appended. Returns whether
    /// it calls helpers.
    fn emit(mut self) -> Option<bool> {
        for (slot, &param) in self.func.params.iter().enumerate() {
            let used = self
                .liveness
                .get(param.0 as usize)
                .is_some_and(|&(start, _)| start != u32::MAX);
            if used {
                let dst = self.def(param, 0)?;
                self.insn(Insn::LoadSlot {
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
                self.def(param, self.pos)?;
            }
            self.pos += 2;
            for instr in &block.instructions {
                self.emit_instr(instr)?;
                self.pos += 2;
            }
            self.emit_term(block.term.as_ref())?;
            self.pos += 2;
        }

        self.patch_jumps()?;
        Some(self.calls)
    }

    fn insn(&mut self, insn: Insn) {
        insn.encode(self.out);
    }

    /// Register of `id`, assigned if this is its first touch. `at` is where it
    /// is first needed; a forward edge needs a block param before its block
    /// starts. Registers are only reclaimed up to the interval start, so the
    /// register is clear of every value the interval overlaps.
    fn def(&mut self, id: ir::Id, at: u32) -> Option<Reg> {
        let (start, last_use) = self.liveness[id.0 as usize];
        match self.ra.alloc(id, at.min(start), last_use) {
            Some(reg) => Some(Reg(reg)),
            None => skip!(self.func, "out of registers at %v{}", id.0),
        }
    }

    fn reg(&self, id: ir::Id) -> Option<Reg> {
        match self.ra.reg(id) {
            Some(reg) => Some(Reg(reg)),
            None => skip!(self.func, "use of %v{} before its definition", id.0),
        }
    }

    /// Run `body` with every register in `clobbers` free to overwrite: values
    /// living in them past this instruction are copied out and back. Copies
    /// are taken before `dst` is assigned so neither the copies nor `dst`
    /// reuse an operand register `body` still reads.
    fn clobbering(
        &mut self,
        clobbers: &[Reg],
        dst: Option<ir::Id>,
        body: impl FnOnce(&mut Self, Option<Reg>),
    ) -> Option<()> {
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
                None => skip!(
                    self.func,
                    "no free register to preserve {} across a clobber",
                    save.0
                ),
            }
        }
        let dst = match dst {
            Some(dst) => Some(self.def(dst, self.pos + 1)?),
            None => None,
        };

        for &(reg, copy) in &saves {
            self.insn(Insn::Mov {
                dst: copy,
                src: reg,
            });
        }
        body(self, dst);
        for &(reg, copy) in &saves {
            self.insn(Insn::Mov {
                dst: reg,
                src: copy,
            });
            self.ra.give_back(copy.0);
        }
        *self.saves = saves;
        Some(())
    }

    fn emit_instr(&mut self, instr: &ir::Instr<'_>) -> Option<()> {
        match instr {
            ir::Instr::Noop => {}
            ir::Instr::Alloc {
                dst: ir::TypeId { id, ty },
                layout,
                ..
            } => {
                let Some(kind) = purple_garden_runtime::AllocType::from_ty(ty) else {
                    skip!(self.func, "unsupported allocation type");
                };
                self.clobbering(CALLER_SAVED, Some(*id), |this, dst| {
                    this.call(
                        purple_garden_runtime::jit_alloc as *const () as u64,
                        &[
                            AbiArg::Reg(VM),
                            AbiArg::Imm(kind as u64),
                            AbiArg::Imm(layout.size() as u64),
                            AbiArg::Imm(layout.align() as u64),
                        ],
                        dst,
                    );
                })?;
            }
            ir::Instr::LoadConst { dst, value, .. } => {
                let Some(imm) = supported_const(value) else {
                    skip!(self.func, "const is not a bool or i32");
                };
                let dst = self.def(dst.id, self.pos + 1)?;
                self.insn(Insn::MovImm { dst, imm });
            }
            ir::Instr::BinImm {
                op, dst, lhs, imm, ..
            } => self.emit_bin_imm(*op, dst.id, *lhs, *imm)?,
            ir::Instr::Bin {
                op, dst, lhs, rhs, ..
            } => self.emit_bin(*op, dst.id, *lhs, *rhs)?,
            ir::Instr::Store {
                src, base, offset, ..
            } if *offset <= i32::MAX as u32 => {
                let (src, base) = (self.reg(*src)?, self.reg(*base)?);
                self.insn(Insn::StoreMem {
                    base,
                    offset: *offset,
                    src,
                });
            }
            ir::Instr::Load {
                dst, base, offset, ..
            } if *offset <= i32::MAX as u32 => {
                let base = self.reg(*base)?;
                let dst = self.def(dst.id, self.pos + 1)?;
                self.insn(Insn::LoadMem {
                    dst,
                    base,
                    offset: *offset,
                });
            }
            ir::Instr::AddrOf {
                dst, base, offset, ..
            } if *offset <= i32::MAX as u32 => {
                let base = self.reg(*base)?;
                let dst = self.def(dst.id, self.pos + 1)?;
                self.insn(Insn::LeaMem {
                    dst,
                    base,
                    offset: *offset,
                });
            }
            _ => skip!(self.func, "unsupported instruction {instr:?}"),
        }
        Some(())
    }

    fn emit_bin_imm(&mut self, op: BinOp, dst: ir::Id, lhs: ir::Id, imm: i32) -> Option<()> {
        let l = self.reg(lhs)?;
        match op {
            BinOp::IAdd | BinOp::ISub => {
                let d = self.def(dst, self.pos + 1)?;
                if d != l {
                    self.insn(Insn::Mov { dst: d, src: l });
                }
                self.insn(match op {
                    BinOp::IAdd => Insn::AddImm { dst: d, imm },
                    _ => Insn::SubImm { dst: d, imm },
                });
            }
            BinOp::IEq => {
                let d = self.def(dst, self.pos + 1)?;
                self.cmp_imm(l, imm);
                self.sete(d);
            }
            BinOp::IDiv | BinOp::IMod if imm == 0 => {
                self.def(dst, self.pos + 1)?;
                self.trap_div_zero();
            }
            BinOp::IMod if imm == 2 => {
                let d = self.def(dst, self.pos + 1)?;
                if d != l {
                    self.insn(Insn::Mov { dst: d, src: l });
                }
                self.insn(Insn::AndImm { dst: d, imm: 1 });
            }
            BinOp::IDiv | BinOp::IMod => self.div(op, dst, l, Divisor::Imm(imm))?,
            _ => skip!(self.func, "unsupported binimm op {op:?}"),
        }
        Some(())
    }

    fn emit_bin(&mut self, op: BinOp, dst: ir::Id, lhs: ir::Id, rhs: ir::Id) -> Option<()> {
        let (l, r) = (self.reg(lhs)?, self.reg(rhs)?);
        match op {
            BinOp::IAdd | BinOp::ISub | BinOp::IMul => {
                let d = self.def(dst, self.pos + 1)?;
                self.two_operand(op, d, l, r);
            }
            BinOp::IEq => {
                let d = self.def(dst, self.pos + 1)?;
                self.insn(Insn::Cmp { lhs: l, rhs: r });
                self.sete(d);
            }
            BinOp::IDiv | BinOp::IMod => {
                self.insn(Insn::Test { lhs: r, rhs: r });
                let nonzero = encode::jump(self.out, Cond::NotZero);
                self.trap_div_zero();
                let resume = self.out.len();
                patch_rel32(self.out, nonzero, resume).expect("short forward jump");
                self.div(op, dst, l, Divisor::Reg(r))?;
            }
            _ => skip!(self.func, "unsupported bin op {op:?}"),
        }
        Some(())
    }

    /// `d = l <op> r` for IAdd/ISub/IMul through x86's two-operand `d <op>= s`.
    fn two_operand(&mut self, op: BinOp, d: Reg, l: Reg, r: Reg) {
        let apply = |dst, src| match op {
            BinOp::IAdd => Insn::Add { dst, src },
            BinOp::ISub => Insn::Sub { dst, src },
            BinOp::IMul => Insn::Imul { dst, src },
            _ => unreachable!("two_operand only lowers IAdd, ISub and IMul"),
        };
        if d == l {
            self.insn(apply(d, r));
        } else if d != r {
            self.insn(Insn::Mov { dst: d, src: l });
            self.insn(apply(d, r));
        } else if op == BinOp::ISub {
            // d holds r: r - l negated is l - r, no temporary needed.
            self.insn(Insn::Sub { dst: d, src: l });
            self.insn(Insn::Neg { reg: d });
        } else {
            self.insn(apply(d, l));
        }
    }

    /// `idiv` divides `rdx:rax`, so the dividend goes through `rax` and `cqo`
    /// clobbers `rdx`. A divisor without a usable register (an immediate, or
    /// one living in `rdx`) is staged in `rcx`.
    ///
    /// `i64::MIN % -1` faults in `idiv` (#DE); the bytecode VM panics on the
    /// same input via Rust's checked `%`, so neither path is lenient here.
    fn div(&mut self, op: BinOp, dst: ir::Id, l: Reg, divisor: Divisor) -> Option<()> {
        let staged = !matches!(divisor, Divisor::Reg(r) if r != RDX);
        let clobbers: &[Reg] = if staged { &[RCX, RDX] } else { &[RDX] };
        self.clobbering(clobbers, Some(dst), |this, d| {
            this.insn(Insn::Mov { dst: RAX, src: l });
            let divisor = match divisor {
                Divisor::Reg(r) if r != RDX => r,
                Divisor::Reg(r) => {
                    this.insn(Insn::Mov { dst: RCX, src: r });
                    RCX
                }
                Divisor::Imm(imm) => {
                    this.insn(Insn::MovImm { dst: RCX, imm });
                    RCX
                }
            };
            this.insn(Insn::Cqo);
            this.insn(Insn::Idiv { divisor });
            if let Some(d) = d {
                let src = if op == BinOp::IDiv { RAX } else { RDX };
                this.insn(Insn::Mov { dst: d, src });
            }
        })
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
        self.insn(Insn::SubImm {
            dst: RSP,
            imm: CALL_STACK,
        });
        self.insn(Insn::StoreMem {
            base: RSP,
            offset: 0,
            src: VM,
        });
        for (&dst, arg) in ARGS.iter().zip(args) {
            match *arg {
                AbiArg::Reg(src) if src != dst => self.insn(Insn::Mov { dst, src }),
                AbiArg::Reg(_) => {}
                AbiArg::Imm(imm) => self.insn(Insn::MovAbs { dst, imm }),
            }
        }
        self.insn(Insn::MovAbs {
            dst: SCRATCH,
            imm: addr,
        });
        self.insn(Insn::CallReg { reg: SCRATCH });
        self.insn(Insn::LoadMem {
            dst: VM,
            base: RSP,
            offset: 0,
        });
        self.insn(Insn::AddImm {
            dst: RSP,
            imm: CALL_STACK,
        });
        if let Some(dst) = result.filter(|&dst| dst != RAX) {
            self.insn(Insn::Mov { dst, src: RAX });
        }
    }

    fn cmp_imm(&mut self, lhs: Reg, imm: i32) {
        if imm == 0 {
            self.insn(Insn::Test { lhs, rhs: lhs });
        } else {
            self.insn(Insn::CmpImm { reg: lhs, imm });
        }
    }

    /// Materialize the equality flag as a full 64-bit boolean.
    fn sete(&mut self, dst: Reg) {
        self.insn(Insn::MovImm { dst, imm: 0 });
        self.insn(Insn::Sete { dst });
    }

    fn emit_term(&mut self, term: Option<&ir::Terminator>) -> Option<()> {
        let pos = self.pos;
        match term {
            None => {}
            Some(ir::Terminator::Return { value, .. }) => {
                if let Some(value) = value {
                    let src = self.reg(*value)?;
                    self.insn(Insn::StoreSlot { src, slot: 0 });
                }
                self.jump(Cond::Always, Target::Epilogue);
            }
            Some(ir::Terminator::Branch { cond, yes, no, .. }) => {
                // Same phasing as liveness: yes moves, then cond, then no moves.
                self.edge(*yes, pos)?;
                let cond = self.reg(*cond)?;
                self.insn(Insn::Test {
                    lhs: cond,
                    rhs: cond,
                });
                self.jump(Cond::NotZero, Target::Block(yes.0));
                self.edge(*no, pos + 1)?;
                self.jump(Cond::Always, Target::Block(no.0));
            }
            Some(ir::Terminator::BranchCmpImm {
                op: BinOp::IEq,
                lhs,
                imm,
                yes,
                no,
                ..
            }) => {
                self.edge(*yes, pos)?;
                let lhs = self.reg(*lhs)?;
                self.cmp_imm(lhs, *imm);
                self.jump(Cond::Zero, Target::Block(yes.0));
                self.edge(*no, pos + 1)?;
                self.jump(Cond::Always, Target::Block(no.0));
            }
            Some(ir::Terminator::Jump { id, params, .. }) => {
                self.edge((*id, *params), pos)?;
                self.jump(Cond::Always, Target::Block(*id));
            }
            Some(ir::Terminator::Tail { func, args, .. }) if *func == self.func.id => {
                self.moves(args, &self.func.params, pos)?;
                self.jump(Cond::Always, Target::Block(self.entry));
            }
            Some(_) => skip!(self.func, "unsupported terminator"),
        }
        Some(())
    }

    /// Resolve the edge's block params, the IR's phi nodes, on the predecessor.
    fn edge(&mut self, (target, params): (ir::Id, ir::ParamsId), at: u32) -> Option<()> {
        let func = self.func;
        let target = &func.blocks[target.0 as usize];
        self.moves(func.params(params), func.params(target.params), at)
    }

    /// Parallel move `src -> dst`, assigning `dst` registers at `at` if this
    /// edge is the first to reach them.
    fn moves(&mut self, src: &[ir::Id], dst: &[ir::Id], at: u32) -> Option<()> {
        debug_assert_eq!(src.len(), dst.len(), "edge arity matches target params");
        let mut pairs = std::mem::take(self.move_pairs);
        pairs.clear();
        for (&s, &d) in src.iter().zip(dst) {
            let s = self.reg(s)?;
            pairs.push((s, self.def(d, at)?));
        }
        self.parallel_moves(&mut pairs);
        *self.move_pairs = pairs;
        Some(())
    }

    /// Emit `pairs` as if every move happened at once.
    fn parallel_moves(&mut self, pairs: &mut Vec<(Reg, Reg)>) {
        pairs.retain(|(src, dst)| src != dst);
        while !pairs.is_empty() {
            let ready = pairs
                .iter()
                .position(|&(_, dst)| !pairs.iter().any(|&(src, _)| src == dst));
            if let Some(i) = ready {
                let (src, dst) = pairs.swap_remove(i);
                self.insn(Insn::Mov { dst, src });
                continue;
            }
            // Only cycles remain, e.g. a->b, b->c, c->a: park a in scratch,
            // then c->a, b->c, scratch->b.
            let (head_src, head_dst) = pairs.swap_remove(0);
            self.insn(Insn::Mov {
                dst: SCRATCH,
                src: head_src,
            });
            let mut freed = head_src;
            while let Some(i) = pairs.iter().position(|&(_, dst)| dst == freed) {
                let (src, dst) = pairs.swap_remove(i);
                self.insn(Insn::Mov { dst, src });
                freed = src;
            }
            self.insn(Insn::Mov {
                dst: head_dst,
                src: SCRATCH,
            });
        }
    }

    fn jump(&mut self, cond: Cond, target: Target) {
        let rel = encode::jump(self.out, cond);
        self.patches.push(Patch { rel, target });
    }

    /// Resolve every jump. The body ends where the epilogue is appended, so a
    /// return jump ending it is dropped and falls through instead.
    fn patch_jumps(&mut self) -> Option<()> {
        if let Some(last) = self.patches.last()
            && last.target == Target::Epilogue
            && last.rel + 4 == self.out.len()
        {
            self.patches.pop();
            self.out.truncate(self.out.len() - 5);
        }
        let end = self.out.len();
        for &Patch { rel, target } in self.patches.iter() {
            let offset = match target {
                Target::Epilogue => end,
                // A block that started at the dropped jump now starts at the epilogue.
                Target::Block(id) => match self.block_offsets[id.0 as usize] {
                    usize::MAX => skip!(self.func, "jump to unemitted block b{}", id.0),
                    offset => offset.min(end),
                },
            };
            patch_rel32(self.out, rel, offset)?;
        }
        Some(())
    }
}
