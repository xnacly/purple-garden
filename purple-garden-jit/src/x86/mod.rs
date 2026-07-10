//! x86-64 JIT lowering.
//!
//! A peer backend to the bytecode compiler: lowers IR straight to native code, with SSA values
//! living in real x86 GPRs (its own [`crate::regalloc`] over the shared IR liveness). The native
//! ABI passes `*mut Vm` in `rdi`, and since `Vm::r` is the first field, `rdi` is the base of the VM
//! register file. Args arrive in `vm.r[0..n]`; the result is written back to `vm.r[0]`. All
//! computation happens in GPRs, `vm.r[1..]` is never touched, so the syscall convention ("a syscall
//! changes only r0") holds for free.
//!
//! This module combines admission, register planning, lowering, and a tiny encoder.
//! The first pre-lowering walk collects constraints that must be known before register allocation.
//! For example, the current `idiv` lowering clobbers `rax`, `rcx`, and `rdx`, so values live across
//! that IR position must be kept out of those registers.
//!
//! The instruction enum contains only the x86-64 forms needed for the current set of pg Ir nodes
//! the x86 jit supports. VM slot access uses `[rdi + slot * 8]`. Record memory access uses generic
//! `[base + offset]` addressing where `offset` is the byte offset carried by IR.
//!
//! Helper calls, such as traps or future allocation support, use the platform C ABI. Such calls
//! clobber caller-saved registers and therefore need admission planning before register allocation

use std::fmt;

use crate::regalloc::FixedClobber;
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

/// Allocatable general purpose: the caller-saved set (so a leaf needs no
/// prologue). `rdi` is the `Vm` pointer, `rsp`/`rbp` the stack. Callee-saved
/// regs (rbx, r12..r15) aren't used yet.
const POOL: &[u8] = &[0, 1, 2, 6, 8, 9, 10, 11]; // rax rcx rdx rsi r8 r9 r10 r11
/// Callee-saved class for values live across a call; the prologue saves the
/// ones actually used.
const POOL_CALLEE: &[u8] = &[3, 12, 13, 14, 15]; // rbx r12 r13 r14 r15
/// `rdi` holds `*mut Vm` == `&vm.r[0]`, the base for slot loads/stores.
const RDI: u8 = 7;
/// `rsp`, the stack pointer; only touched to re-align for an ABI call.
const RSP: u8 = 4;
const RAX: u8 = 0;
const RCX: u8 = 1;
const RDX: u8 = 2;
/// Registers implicitly clobbered by the current `idiv` lowering.
const IDIV_CLOBBERS: &[u8] = &[RAX, RCX, RDX];

#[derive(Default)]
struct SupportPlan {
    entry: Option<ir::Id>,
    /// Target-specific register hazards discovered before allocation. Each
    /// entry says that a lowering at `pos` overwrites `regs`; the allocator then
    /// avoids assigning live-across values to those registers.
    fixed_clobbers: Vec<FixedClobber<'static>>,
}

/// Compile one IR function into x86-64 machine code, returning `None` if unsupported.
pub fn compile_func(
    func: &ir::Func<'_>,
    out: &mut Vec<u8>,
    liveness: &[(u32, u32)],
    allocator: &mut crate::regalloc::Allocator,
) -> Option<()> {
    if func.params.len() > 32 {
        skip!(
            func,
            "too many params for disp8 slot loads: {}",
            func.params.len()
        );
    }

    let plan = validate_supported(func)?;
    let Some(entry) = plan.entry else {
        skip!(func, "empty function");
    };

    out.reserve(func.params.len() * 4 + func.blocks.len() * 16 + 64);

    let regs = allocator.rebuild(
        liveness,
        &[],
        &plan.fixed_clobbers,
        crate::regalloc::RegClasses {
            caller: POOL,
            callee: POOL_CALLEE,
        },
    );
    // Parallel edge moves only need a scratch register for cycles. If the
    // allocator consumed every caller register, cyclic edge moves are rejected
    // and the function falls back to bytecode.
    let scratch = POOL.iter().copied().find(|candidate| {
        !regs
            .iter()
            .any(|loc| matches!(loc, ir::Location::Reg(r) if r == candidate))
    });
    Lowering::new(func, out, regs, scratch, entry).emit()?;

    // we have produced no machine code, so we just RET, this may be the case for fully optimised
    // (dce) away IR
    if out.is_empty() {
        Insn::Ret.encode(out);
    }

    purple_garden_shared::trace!("[jit::x86] compiled {} ({} bytes)", func.name, out.len());
    Some(())
}

#[derive(Clone, Copy)]
struct Patch {
    /// Offset of the 4-byte relative displacement inside `out`.
    rel: usize,
    /// IR block id that owns the final machine-code target offset.
    target: ir::Id,
}

/// A single x86-64 instruction. `encode` appends its machine-code bytes;
/// `Display` renders it as readable assembly (the JIT's own disassembler).
///
/// Register fields use x86's physical GPR numbering:
///
/// ```text
/// 0 rax   1 rcx   2 rdx   3 rbx   4 rsp   5 rbp   6 rsi   7 rdi
/// 8 r8    9 r9   10 r10  11 r11  12 r12  13 r13  14 r14  15 r15
/// ```
///
/// The low three bits go into ModRM/SIB fields. Bit 3 is carried by a REX
/// prefix (`REX.R` for the ModRM `reg` field, `REX.B` for the ModRM `r/m`
/// field, or opcode low bits for `movabs`). `slot` indexes the VM register file
/// at `[rdi + slot*8]`.
#[derive(Debug, Clone, Copy)]
pub enum Insn {
    Ret,
    /// `mov r{dst}, [rdi + slot*8]`
    LoadSlot {
        dst: u8,
        slot: u8,
    },
    /// `mov [rdi + slot*8], r{src}`
    StoreSlot {
        src: u8,
        slot: u8,
    },
    /// `mov r{dst}, [r{base} + offset]`
    LoadMem {
        dst: u8,
        base: u8,
        offset: u32,
    },
    /// `mov [r{base} + offset], r{src}`
    StoreMem {
        base: u8,
        offset: u32,
        src: u8,
    },
    /// `lea r{dst}, [r{base} + offset]`
    LeaMem {
        dst: u8,
        base: u8,
        offset: u32,
    },
    /// `mov r{dst}, r{src}`
    Mov {
        dst: u8,
        src: u8,
    },
    /// `mov r{dst}, imm` (sign-extended into 64 bits)
    MovImm {
        dst: u8,
        imm: i32,
    },
    /// `add r{dst}, r{src}`
    Add {
        dst: u8,
        src: u8,
    },
    /// `sub r{dst}, r{src}`
    Sub {
        dst: u8,
        src: u8,
    },
    /// `imul r{dst}, r{src}`
    Imul {
        dst: u8,
        src: u8,
    },
    /// `neg r{reg}` (two's-complement negate)
    Neg {
        reg: u8,
    },
    /// `add r{dst}, imm`
    AddImm {
        dst: u8,
        imm: i32,
    },
    /// `sub r{dst}, imm`
    SubImm {
        dst: u8,
        imm: i32,
    },
    /// `and r{dst}, imm`
    AndImm {
        dst: u8,
        imm: i32,
    },
    /// `cmp r{reg}, imm`
    CmpImm {
        reg: u8,
        imm: i32,
    },
    /// `cmp r{lhs}, r{rhs}`
    Cmp {
        lhs: u8,
        rhs: u8,
    },
    /// `test r{lhs}, r{rhs}`
    Test {
        lhs: u8,
        rhs: u8,
    },
    /// `sete r{dst}b`; set r{dst}'s low byte to 1 if the last compare was equal.
    Sete {
        dst: u8,
    },
    /// `movabs r{dst}, imm64`; `MovImm` is i32-only, addresses need 64 bits.
    MovAbs {
        dst: u8,
        imm: u64,
    },
    /// `call r{reg}`
    CallReg {
        reg: u8,
    },
    /// `cqo`; sign-extend rax into rdx:rax (the idiv dividend).
    Cqo,
    /// `idiv r{divisor}`; rdx:rax / divisor, quotient to rax, remainder to rdx.
    Idiv {
        divisor: u8,
    },
}

impl Insn {
    /// Append this instruction's x86-64 machine-code bytes to `code`.
    pub fn encode(self, code: &mut Vec<u8>) {
        match self {
            Insn::Ret => code.push(0xc3),
            Insn::LoadSlot { dst, slot } => mov_slot(code, 0x8b, dst, slot),
            Insn::StoreSlot { src, slot } => mov_slot(code, 0x89, src, slot),
            // Record memory ops all use the same ModRM/SIB memory form. The
            // opcode selects load, store, or address calculation; ModRM.reg is
            // the register operand for all three encodings.
            Insn::LoadMem { dst, base, offset } => mem_disp(code, 0x8b, dst, base, offset),
            Insn::StoreMem { base, offset, src } => mem_disp(code, 0x89, src, base, offset),
            Insn::LeaMem { dst, base, offset } => mem_disp(code, 0x8d, dst, base, offset),
            // 0x89 = `mov r/m64, r64`.
            // ModRM.reg encodes src; ModRM.r/m encodes dst.
            Insn::Mov { dst, src } => reg_reg(code, 0x89, src, dst),
            // 0x01 = `add r/m64, r64`, 0x29 = `sub r/m64, r64`.
            // Same direction as mov: reg is src, r/m is dst.
            Insn::Add { dst, src } => reg_reg(code, 0x01, src, dst),
            Insn::Sub { dst, src } => reg_reg(code, 0x29, src, dst),
            // 0x0f 0xaf = `imul r64, r/m64`.
            // Here ModRM.reg is dst and ModRM.r/m is src, opposite of add/sub.
            Insn::Imul { dst, src } => {
                code.push(rex(dst, src));
                code.extend_from_slice(&[0x0f, 0xaf, modrm(dst, src)]);
            }
            // 0xf7 /3 = `neg r/m64`.
            // `/3` means ModRM.reg is not a register; it is the opcode extension
            // digit 3. ModRM.r/m names the operand.
            Insn::Neg { reg } => {
                code.push(rex(0, reg));
                code.extend_from_slice(&[0xf7, modrm(3, reg)]);
            }
            // 0x81 /digit = `op r/m64, imm32`.
            // /0 add, /4 and, /5 sub, /7 cmp.
            Insn::AddImm { dst, imm } => reg_imm(code, 0, dst, imm),
            Insn::SubImm { dst, imm } => reg_imm(code, 5, dst, imm),
            Insn::AndImm { dst, imm } => reg_imm(code, 4, dst, imm),
            Insn::CmpImm { reg, imm } => reg_imm(code, 7, reg, imm),
            Insn::Cmp { lhs, rhs } => reg_reg(code, 0x39, rhs, lhs),
            // 0x85 = `test r/m64, r64`.
            // Both operands are only read, but keep the same packing convention:
            // ModRM.reg = rhs, ModRM.r/m = lhs.
            Insn::Test { lhs, rhs } => reg_reg(code, 0x85, rhs, lhs),
            // 0xc7 /0 = `mov r/m64, imm32`.
            Insn::MovImm { dst, imm } => {
                code.push(rex(0, dst));
                code.push(0xc7);
                code.push(modrm(0, dst));
                code.extend_from_slice(&imm.to_le_bytes());
            }
            // 0x0f 0x94 = `sete r/m8`.
            // The REX prefix is not REX.W here; it exists only so byte-register
            // names are the modern low-byte registers (`sil`, `dil`, `r8b`, ...).
            // ModRM.reg is /0, ModRM.r/m names the byte destination.
            Insn::Sete { dst } => {
                code.push(0x40 | u8::from(dst >= 8));
                code.extend_from_slice(&[0x0f, 0x94, modrm(0, dst)]);
            }
            // REX.W 0xb8+rd io64 = `movabs r64, imm64`.
            // This form has no ModRM byte; the low 3 register bits are embedded
            // in the opcode and the high bit goes in REX.B.
            Insn::MovAbs { dst, imm } => {
                code.push(0x48 | u8::from(dst >= 8));
                code.push(0xb8 + (dst & 7));
                code.extend_from_slice(&imm.to_le_bytes());
            }
            // 0xff /2 = `call r/m64`.
            // `/2` is the opcode extension; ModRM.r/m names the call target.
            // No REX.W is required. REX.B is enough to reach r8..r15.
            Insn::CallReg { reg } => {
                if reg >= 8 {
                    code.push(0x41);
                }
                code.extend_from_slice(&[0xff, modrm(2, reg)]);
            }
            // REX.W 0x99 ; cqo.
            Insn::Cqo => code.extend_from_slice(&[0x48, 0x99]),
            // REX.W 0xf7 /7 = `idiv r/m64`.
            // `/7` is the opcode extension; ModRM.r/m names the divisor.
            Insn::Idiv { divisor } => {
                code.push(rex(0, divisor));
                code.extend_from_slice(&[0xf7, modrm(7, divisor)]);
            }
        }
    }
}

#[inline]
/// Append one typed instruction to the output buffer.
fn emit(code: &mut Vec<u8>, insn: Insn) {
    insn.encode(code);
}

/// Emit a near unconditional jump with a zero rel32 placeholder.
fn emit_jmp_placeholder(code: &mut Vec<u8>) -> usize {
    code.push(0xe9);
    let rel = code.len();
    code.extend_from_slice(&0i32.to_le_bytes());
    rel
}

/// Emit a near `jnz` with a zero rel32 placeholder.
fn emit_jnz_placeholder(code: &mut Vec<u8>) -> usize {
    code.extend_from_slice(&[0x0f, 0x85]);
    let rel = code.len();
    code.extend_from_slice(&0i32.to_le_bytes());
    rel
}

/// Emit a near `jz` with a zero rel32 placeholder.
fn emit_jz_placeholder(code: &mut Vec<u8>) -> usize {
    code.extend_from_slice(&[0x0f, 0x84]);
    let rel = code.len();
    code.extend_from_slice(&0i32.to_le_bytes());
    rel
}

/// Patch a rel32 branch displacement.
///
/// x86 rel32 offsets are relative to the instruction end, not the displacement
/// field itself. `rel` points at the first byte of the placeholder disp32.
fn patch_rel32(code: &mut [u8], rel: usize, target: usize) -> Option<()> {
    let next = rel.checked_add(4)?;
    let disp = target as isize - next as isize;
    let disp = i32::try_from(disp).ok()?;
    code[rel..next].copy_from_slice(&disp.to_le_bytes());
    Some(())
}

/// 64-bit GPR name for a physical register number.
fn reg_name(r: u8) -> &'static str {
    [
        "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12",
        "r13", "r14", "r15",
    ][r as usize]
}

impl fmt::Display for Insn {
    /// Render one instruction as the JIT's readable assembly format.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let r = reg_name;
        match *self {
            Insn::Ret => write!(f, "ret"),
            Insn::LoadSlot { dst, slot } => write!(f, "mov {}, [rdi+{:#x}]", r(dst), slot * 8),
            Insn::StoreSlot { src, slot } => write!(f, "mov [rdi+{:#x}], {}", slot * 8, r(src)),
            Insn::LoadMem { dst, base, offset } => {
                write!(f, "mov {}, [{}+{:#x}]", r(dst), r(base), offset)
            }
            Insn::StoreMem { base, offset, src } => {
                write!(f, "mov [{}+{:#x}], {}", r(base), offset, r(src))
            }
            Insn::LeaMem { dst, base, offset } => {
                write!(f, "lea {}, [{}+{:#x}]", r(dst), r(base), offset)
            }
            Insn::Mov { dst, src } => write!(f, "mov {}, {}", r(dst), r(src)),
            Insn::MovImm { dst, imm } => write!(f, "mov {}, {imm}", r(dst)),
            Insn::Add { dst, src } => write!(f, "add {}, {}", r(dst), r(src)),
            Insn::Sub { dst, src } => write!(f, "sub {}, {}", r(dst), r(src)),
            Insn::Imul { dst, src } => write!(f, "imul {}, {}", r(dst), r(src)),
            Insn::Neg { reg } => write!(f, "neg {}", r(reg)),
            Insn::AddImm { dst, imm } => write!(f, "add {}, {imm}", r(dst)),
            Insn::SubImm { dst, imm } => write!(f, "sub {}, {imm}", r(dst)),
            Insn::AndImm { dst, imm } => write!(f, "and {}, {imm}", r(dst)),
            Insn::CmpImm { reg, imm } => write!(f, "cmp {}, {imm}", r(reg)),
            Insn::Cmp { lhs, rhs } => write!(f, "cmp {}, {}", r(lhs), r(rhs)),
            Insn::Test { lhs, rhs } => write!(f, "test {}, {}", r(lhs), r(rhs)),
            Insn::Sete { dst } => write!(f, "sete {}b", r(dst)),
            Insn::MovAbs { dst, imm } => write!(f, "movabs {}, {imm:#x}", r(dst)),
            Insn::CallReg { reg } => write!(f, "call {}", r(reg)),
            Insn::Cqo => write!(f, "cqo"),
            Insn::Idiv { divisor } => write!(f, "idiv {}", r(divisor)),
        }
    }
}

/// REX.W prefix for 64-bit operand size, plus high register bits.
///
/// ```text
/// 0100WRXB
///     ||||
///     |||+-- B: high bit for ModRM.r/m, SIB.base, or opcode +rd
///     ||+--- X: high bit for SIB.index (unused here)
///     |+---- R: high bit for ModRM.reg
///     +----- W: 64-bit operand size
/// ```
///
/// This backend only needs `W`, `R`, and `B`, so the base byte is `0x48`
/// (`0100_1000`: REX.W) and we OR in `R`/`B` from register numbers >= 8.
fn rex(reg: u8, rm: u8) -> u8 {
    0x48 | (u8::from(reg >= 8) << 2) | u8::from(rm >= 8)
}

/// ModRM byte for register-direct operands.
///
/// ```text
/// 76543210
/// mmrrrbbb
/// ||||||||
/// |||||+++-- r/m: operand register low 3 bits
/// ||+++----- reg: register operand low 3 bits, or an opcode extension `/digit`
/// ++-------- mod: addressing mode; `11` means register-direct
/// ```
///
/// For example `modrm(1, 0)` with opcode `0x89` means `mov rax, rcx`:
/// `reg=rcx`, `r/m=rax`, `mod=11`.
fn modrm(reg: u8, rm: u8) -> u8 {
    0xc0 | ((reg & 7) << 3) | (rm & 7)
}

/// Register-register op: `REX.W opcode ModRM(reg, rm)`.
///
/// The meaning of `reg` and `rm` depends on the opcode:
///
/// - `mov/add/sub r/m64, r64`: `rm` is dst, `reg` is src.
/// - `imul r64, r/m64`: `reg` is dst, `rm` is src.
/// - `test r/m64, r64`: both are sources.
fn reg_reg(code: &mut Vec<u8>, opcode: u8, reg: u8, rm: u8) {
    code.extend_from_slice(&[rex(reg, rm), opcode, modrm(reg, rm)]);
}

/// Register-immediate op: `REX.W 0x81 /digit r/m64, imm32`.
///
/// The `/digit` is encoded in ModRM.reg and selects the operation:
/// `/0 add`, `/4 and`, `/5 sub`, `/7 cmp`. The actual destination register is
/// ModRM.r/m.
fn reg_imm(code: &mut Vec<u8>, digit: u8, rm: u8, imm: i32) {
    code.push(rex(0, rm));
    code.push(0x81);
    code.push(modrm(digit, rm));
    code.extend_from_slice(&imm.to_le_bytes());
}

/// `mov` between GPR `reg` and `[rdi + slot*8]` (opcode 0x8b load, 0x89 store).
///
/// Memory operands use `mod != 11`, so this cannot use [`modrm`]. We use:
///
/// ```text
/// mod = 01       disp8 follows the ModRM byte
/// reg = reg&7    loaded/stored GPR
/// r/m = 111      base register rdi
/// disp8 = slot*8 byte offset into Vm::r
/// ```
///
/// This is why `compile_func` currently rejects more than 32 params: `slot*8`
/// must fit in a signed 8-bit displacement for this compact addressing form.
fn mov_slot(code: &mut Vec<u8>, opcode: u8, reg: u8, slot: u8) {
    // ModRM mod=01 (disp8), reg field = GPR, rm = rdi.
    let m = 0x40 | ((reg & 7) << 3) | RDI;
    code.extend_from_slice(&[rex(reg, RDI), opcode, m, slot * 8]);
}

/// Memory op using `[base + offset]`, where ModRM.reg is the register operand
/// and ModRM.r/m names the base. Always emits an explicit displacement (disp8
/// when possible, otherwise disp32), which avoids zero-offset special cases for
/// rbp/r13. rsp/r12 bases require a SIB byte even with no index.
///
/// This helper is intended for record payload access. IR offsets are already
/// byte offsets, unlike VM register slots, so callers pass the offset through
/// unchanged.
fn mem_disp(code: &mut Vec<u8>, opcode: u8, reg: u8, base: u8, offset: u32) {
    let disp8 = u8::try_from(offset)
        .ok()
        .filter(|offset| *offset <= i8::MAX as u8);
    let mode = if disp8.is_some() { 0x40 } else { 0x80 };
    // In ModRM, r/m=100 does not mean `rsp` directly; it means a SIB byte
    // follows. That is mandatory for rsp/r12 bases, even without an index.
    let rm = if needs_sib(base) { RSP } else { base & 7 };
    let m = mode | ((reg & 7) << 3) | rm;

    code.extend_from_slice(&[rex(reg, base), opcode, m]);
    if needs_sib(base) {
        code.push(sib_no_index(base));
    }
    if let Some(disp) = disp8 {
        code.push(disp);
    } else {
        code.extend_from_slice(&offset.to_le_bytes());
    }
}

fn needs_sib(base: u8) -> bool {
    base & 7 == RSP
}

fn sib_no_index(base: u8) -> u8 {
    // scale=0, index=100 (none), base=base low bits.
    0x20 | (base & 7)
}

/// Emit `r{d} = r{l} <op> r{r}` in place (op is IAdd/ISub/IMul). x86 binops are
/// two-operand (`dst <op>= src`), so the destination must start out holding the
/// left operand; the branches handle the cases where the allocator gave the
/// result the same register as an operand.
fn emit_bin(out: &mut Vec<u8>, op: BinOp, d: u8, l: u8, r: u8) {
    if d == l {
        // dst already holds lhs.
        op_in_place(out, op, d, r);
    } else if d == r {
        // dst holds rhs. add/mul commute, so `dst <op>= lhs` is the answer. sub
        // doesn't: compute rhs - lhs, then negate -> lhs - rhs (no temp needed).
        if matches!(op, BinOp::ISub) {
            emit(out, Insn::Sub { dst: d, src: l });
            emit(out, Insn::Neg { reg: d });
        } else {
            op_in_place(out, op, d, l);
        }
    } else {
        // dst aliases neither operand: load lhs, then op rhs.
        emit(out, Insn::Mov { dst: d, src: l });
        op_in_place(out, op, d, r);
    }
}

/// C-ABI `call addr`, rdi already holding `*mut Vm`. Leaves enter at `rsp % 16
/// == 8`, so realign with `sub`/`add rsp, 8`. Callees clobber caller-saved regs,
/// fine here: the only use is a trap callback that returns right after.
fn emit_abi_call(out: &mut Vec<u8>, addr: u64) {
    emit(out, Insn::SubImm { dst: RSP, imm: 8 });
    emit(out, Insn::MovAbs { dst: 0, imm: addr }); // rax = addr
    emit(out, Insn::CallReg { reg: 0 }); // call rax
    emit(out, Insn::AddImm { dst: RSP, imm: 8 });
}

/// `d = l <op> imm` for IDiv/IMod, nonzero constant divisor. idiv has no imm
/// form, so the divisor goes via rcx. rax/rcx/rdx are clobbered here; the
/// allocator keeps values live across this position out of them via
/// `IDIV_CLOBBERS`. l (consumed into rax first) and d (written last) may reuse
/// them.
fn emit_idiv(out: &mut Vec<u8>, op: BinOp, d: u8, l: u8, imm: i32) {
    emit(out, Insn::Mov { dst: RAX, src: l });
    emit(out, Insn::Cqo);
    emit(out, Insn::MovImm { dst: RCX, imm });
    emit(out, Insn::Idiv { divisor: RCX });
    let src = if matches!(op, BinOp::IDiv) { RAX } else { RDX };
    emit(out, Insn::Mov { dst: d, src });
}

/// `r{d} <op>= r{s}` for IAdd/ISub/IMul.
fn op_in_place(out: &mut Vec<u8>, op: BinOp, d: u8, s: u8) {
    emit(
        out,
        match op {
            BinOp::IAdd => Insn::Add { dst: d, src: s },
            BinOp::ISub => Insn::Sub { dst: d, src: s },
            BinOp::IMul => Insn::Imul { dst: d, src: s },
            _ => unreachable!("emit_bin only handles IAdd/ISub/IMul"),
        },
    );
}

/// Emit a register shuffle, resolving cycles through `scratch` when needed.
fn emit_parallel_moves(out: &mut Vec<u8>, pairs: &mut Vec<(u8, u8)>, scratch: Option<u8>) -> bool {
    pairs.retain(|(src, dst)| src != dst);

    'outer: loop {
        if pairs.is_empty() {
            return true;
        }

        for i in 0..pairs.len() {
            let (src, dst) = pairs[i];
            if !pairs.iter().any(|(other_src, _)| *other_src == dst) {
                emit(out, Insn::Mov { dst, src });
                pairs.swap_remove(i);
                continue 'outer;
            }
        }

        // Every remaining dst is also a src: the moves contain a cycle.
        // Break one cycle by saving its head in a free scratch register, then
        // walk backwards through the just-freed destination registers.
        let Some(scratch) = scratch else {
            return false;
        };
        if pairs
            .iter()
            .any(|(src, dst)| *src == scratch || *dst == scratch)
        {
            return false;
        }

        // Example cycle: a->b, b->c, c->a.
        // Save `a` in scratch, then move c->a, b->c, scratch->b.
        let (start_src, start_dst) = pairs.swap_remove(0);
        emit(
            out,
            Insn::Mov {
                dst: scratch,
                src: start_src,
            },
        );
        let mut cur_freed = start_src;
        while let Some(idx) = pairs.iter().position(|(_, dst)| *dst == cur_freed) {
            let (src, dst) = pairs.swap_remove(idx);
            emit(out, Insn::Mov { dst, src });
            cur_freed = src;
        }
        emit(
            out,
            Insn::Mov {
                dst: start_dst,
                src: scratch,
            },
        );
    }
}

/// Return the physical register allocated to an IR value.
fn reg_of(regs: &[ir::Location], id: ir::Id) -> Option<u8> {
    match regs.get(id.0 as usize) {
        Some(ir::Location::Reg(r)) => Some(*r),
        _ => None,
    }
}

/// Move source edge arguments into destination block parameter registers.
fn emit_param_moves(
    out: &mut Vec<u8>,
    regs: &[ir::Location],
    pairs: &mut Vec<(u8, u8)>,
    scratch: Option<u8>,
    src_params: &[ir::Id],
    dst_params: &[ir::Id],
) -> bool {
    if src_params.len() != dst_params.len() {
        return false;
    }

    pairs.clear();
    for (&src, &dst) in src_params.iter().zip(dst_params) {
        let (Some(src), Some(dst)) = (reg_of(regs, src), reg_of(regs, dst)) else {
            return false;
        };
        pairs.push((src, dst));
    }
    emit_parallel_moves(out, pairs, scratch)
}

/// Return the parameter ids owned by a branch target block.
fn branch_target_params<'f>(func: &'f ir::Func<'_>, target: ir::Id) -> Option<&'f [ir::Id]> {
    func.blocks
        .get(target.0 as usize)
        .map(|block| func.params(block.params))
}

/// Whether this immediate binary op has a direct x86 lowering.
fn supported_bin_imm(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::IAdd | BinOp::ISub | BinOp::IEq | BinOp::IDiv | BinOp::IMod
    )
}

/// Whether this register-register binary op has a direct x86 lowering.
fn supported_bin(op: BinOp) -> bool {
    matches!(op, BinOp::IAdd | BinOp::ISub | BinOp::IMul | BinOp::IEq)
}

/// Whether a constant can be materialized by the current x86 lowering.
fn supported_const(value: &ir::Const<'_>) -> bool {
    match value {
        ir::Const::False | ir::Const::True => true,
        ir::Const::Int(i) => (*i as i32) < i32::MAX && (*i as i32) > i32::MIN,
        _ => false,
    }
}

fn supported_mem_offset(offset: u32) -> bool {
    offset <= i32::MAX as u32
}

/// Validate that `func` is in the x86 JIT subset and collect lowering constraints.
fn validate_supported(func: &ir::Func<'_>) -> Option<SupportPlan> {
    let mut plan = SupportPlan::default();
    let mut pos = 0;

    for block in func.blocks.iter().filter(|block| !block.tombstone) {
        plan.entry.get_or_insert(block.id);

        // Keep this position walk in lockstep with `Func::live_set_into`.
        pos += 2;

        for instr in &block.instructions {
            match instr {
                ir::Instr::Noop => {}
                ir::Instr::LoadConst { value, .. } if supported_const(value) => {}
                ir::Instr::BinImm { op, imm, .. } if supported_bin_imm(*op) => {
                    if matches!(op, BinOp::IDiv if *imm != 0)
                        || matches!(op, BinOp::IMod if *imm != 0 && *imm != 2)
                    {
                        plan.fixed_clobbers.push(FixedClobber {
                            pos,
                            regs: IDIV_CLOBBERS,
                        });
                    }
                }
                ir::Instr::Bin { op, .. } if supported_bin(*op) => {}
                ir::Instr::Store { offset, .. }
                | ir::Instr::Load { offset, .. }
                | ir::Instr::AddrOf { offset, .. }
                    if supported_mem_offset(*offset) => {}
                _ => skip!(func, "unsupported instruction {instr:?}"),
            }
            pos += 2;
        }

        match block.term.as_ref() {
            None | Some(ir::Terminator::Return { .. } | ir::Terminator::Branch { .. }) => {}
            Some(ir::Terminator::BranchCmpImm { op: BinOp::IEq, .. }) => {}
            Some(ir::Terminator::Tail {
                func: tail_func, ..
            }) if *tail_func == func.id => {}
            Some(ir::Terminator::Jump { .. }) => {}
            Some(_) => skip!(func, "unsupported terminator"),
        }
        pos += 2;
    }

    Some(plan)
}

/// Per-function x86 lowering state.
///
/// `compile_func` performs admission and register allocation, then hands the
/// mutable emission state here. Keeping CFG patching, edge-param shuffles, and
/// instruction lowering in methods keeps the public entry point small.
struct Lowering<'a, 'ir> {
    func: &'a ir::Func<'ir>,
    out: &'a mut Vec<u8>,
    regs: &'a [ir::Location],
    scratch: Option<u8>,
    entry: ir::Id,
    block_offsets: Vec<usize>,
    patches: Vec<Patch>,
    move_pairs: Vec<(u8, u8)>,
}

impl<'a, 'ir> Lowering<'a, 'ir> {
    /// Create lowering state for one already-admitted IR function.
    fn new(
        func: &'a ir::Func<'ir>,
        out: &'a mut Vec<u8>,
        regs: &'a [ir::Location],
        scratch: Option<u8>,
        entry: ir::Id,
    ) -> Self {
        Self {
            func,
            out,
            regs,
            scratch,
            entry,
            block_offsets: vec![usize::MAX; func.blocks.len()],
            patches: Vec::new(),
            move_pairs: Vec::new(),
        }
    }

    /// Emit the full function body, then patch deferred branch displacements.
    fn emit(mut self) -> Option<()> {
        self.emit_entry_loads();

        for block in &self.func.blocks {
            if block.tombstone {
                continue;
            }

            self.block_offsets[block.id.0 as usize] = self.out.len();

            for instr in &block.instructions {
                self.emit_instr(instr)?;
            }
            self.emit_term(block.term.as_ref())?;
        }

        self.patch_jumps()
    }

    /// Load function parameters from `vm.r` slots into allocated registers.
    fn emit_entry_loads(&mut self) {
        // Args arrive in the VM register file: param i in vm.r[i] == [rdi + i*8].
        for (i, &param) in self.func.params.iter().enumerate() {
            if let Some(r) = reg_of(self.regs, param) {
                emit(
                    self.out,
                    Insn::LoadSlot {
                        dst: r,
                        slot: i as u8,
                    },
                );
            }
        }
    }

    /// Emit one supported IR instruction.
    fn emit_instr(&mut self, instr: &ir::Instr<'_>) -> Option<()> {
        match instr {
            ir::Instr::Noop => {}
            ir::Instr::LoadConst { dst, value, .. } => self.emit_const(dst, value)?,
            ir::Instr::BinImm {
                op, dst, lhs, imm, ..
            } => self.emit_bin_imm(*op, dst.id, *lhs, *imm)?,
            ir::Instr::Bin {
                op, dst, lhs, rhs, ..
            } => self.emit_bin(*op, dst.id, *lhs, *rhs)?,
            ir::Instr::Store {
                src, base, offset, ..
            } => self.emit_store(*src, *base, *offset)?,
            ir::Instr::Load {
                dst, base, offset, ..
            } => self.emit_load(dst.id, *base, *offset)?,
            ir::Instr::AddrOf {
                dst, base, offset, ..
            } => self.emit_addrof(dst.id, *base, *offset)?,
            _ => skip!(self.func, "unsupported instruction {instr:?}"),
        }
        Some(())
    }

    fn emit_store(&mut self, src: ir::Id, base: ir::Id, offset: u32) -> Option<()> {
        let (Some(src), Some(base)) = (reg_of(self.regs, src), reg_of(self.regs, base)) else {
            skip!(self.func, "unallocated store operand");
        };
        emit(self.out, Insn::StoreMem { base, offset, src });
        Some(())
    }

    fn emit_load(&mut self, dst: ir::Id, base: ir::Id, offset: u32) -> Option<()> {
        let (Some(dst), Some(base)) = (reg_of(self.regs, dst), reg_of(self.regs, base)) else {
            skip!(self.func, "unallocated load operand");
        };
        emit(self.out, Insn::LoadMem { dst, base, offset });
        Some(())
    }

    fn emit_addrof(&mut self, dst: ir::Id, base: ir::Id, offset: u32) -> Option<()> {
        let (Some(dst), Some(base)) = (reg_of(self.regs, dst), reg_of(self.regs, base)) else {
            skip!(self.func, "unallocated addrof operand");
        };
        emit(self.out, Insn::LeaMem { dst, base, offset });
        Some(())
    }

    /// Materialize a supported IR constant into its allocated destination.
    fn emit_const(&mut self, dst: &ir::TypeId<'_>, value: &ir::Const<'_>) -> Option<()> {
        let Some(dst_reg) = reg_of(self.regs, dst.id) else {
            skip!(self.func, "unallocated const dst %v{}", dst.id.0);
        };
        let imm = match value {
            ir::Const::False => 0,
            ir::Const::True => 1,
            ir::Const::Int(i) if (*i as i32) < i32::MAX && (*i as i32) > i32::MIN => *i as i32,
            _ => skip!(
                self.func,
                "const not true, false or i32::MIN < i < i32::MAX"
            ),
        };
        emit(self.out, Insn::MovImm { dst: dst_reg, imm });
        Some(())
    }

    /// Emit an immediate binary operation.
    fn emit_bin_imm(&mut self, op: BinOp, dst: ir::Id, lhs: ir::Id, imm: i32) -> Option<()> {
        let (Some(d), Some(l)) = (reg_of(self.regs, dst), reg_of(self.regs, lhs)) else {
            skip!(self.func, "unallocated binimm operand");
        };

        match op {
            BinOp::IAdd | BinOp::ISub => {
                if d != l {
                    emit(self.out, Insn::Mov { dst: d, src: l });
                }
                emit(
                    self.out,
                    match op {
                        BinOp::IAdd => Insn::AddImm { dst: d, imm },
                        _ => Insn::SubImm { dst: d, imm },
                    },
                );
            }
            BinOp::IEq => self.emit_int_eq_imm(d, l, imm),
            BinOp::IDiv | BinOp::IMod if imm == 0 => {
                let helper: purple_garden_runtime::BuiltinFn =
                    purple_garden_runtime::jit_trap_div_zero;
                emit_abi_call(self.out, helper as usize as u64);
                emit(self.out, Insn::Ret);
            }
            BinOp::IMod if imm == 2 => {
                if d != l {
                    emit(self.out, Insn::Mov { dst: d, src: l });
                }
                emit(self.out, Insn::AndImm { dst: d, imm: 1 });
            }
            BinOp::IDiv | BinOp::IMod => emit_idiv(self.out, op, d, l, imm),
            _ => skip!(self.func, "unsupported binimm op {op:?}"),
        }
        Some(())
    }

    /// Emit a register-register binary operation.
    fn emit_bin(&mut self, op: BinOp, dst: ir::Id, lhs: ir::Id, rhs: ir::Id) -> Option<()> {
        let (Some(d), Some(l), Some(r)) = (
            reg_of(self.regs, dst),
            reg_of(self.regs, lhs),
            reg_of(self.regs, rhs),
        ) else {
            skip!(self.func, "unallocated bin operand");
        };

        match op {
            BinOp::IAdd | BinOp::ISub | BinOp::IMul => emit_bin(self.out, op, d, l, r),
            BinOp::IEq => self.emit_int_eq(d, l, r),
            _ => skip!(self.func, "unsupported bin op {op:?}"),
        }
        Some(())
    }

    /// Emit `dst = lhs == imm` as `test`/`cmp` plus `sete`.
    fn emit_int_eq_imm(&mut self, dst: u8, lhs: u8, imm: i32) {
        if imm == 0 {
            emit(self.out, Insn::Test { lhs, rhs: lhs });
        } else {
            emit(self.out, Insn::CmpImm { reg: lhs, imm });
        }
        self.emit_sete(dst);
    }

    /// Emit `dst = lhs == rhs` as `cmp` plus `sete`.
    fn emit_int_eq(&mut self, dst: u8, lhs: u8, rhs: u8) {
        emit(self.out, Insn::Cmp { lhs, rhs });
        self.emit_sete(dst);
    }

    /// Materialize the current equality flag into a full 64-bit boolean value.
    fn emit_sete(&mut self, dst: u8) {
        emit(self.out, Insn::MovImm { dst, imm: 0 });
        emit(self.out, Insn::Sete { dst });
    }

    fn emit_term(&mut self, term: Option<&ir::Terminator>) -> Option<()> {
        match term {
            None => {}
            Some(ir::Terminator::Return { value, .. }) => self.emit_return(*value)?,
            Some(ir::Terminator::Branch {
                cond,
                yes: (yes_id, yes_params),
                no: (no_id, no_params),
                ..
            }) => self.emit_branch(*cond, *yes_id, *yes_params, *no_id, *no_params)?,
            Some(ir::Terminator::BranchCmpImm {
                op,
                lhs,
                imm,
                yes: (yes_id, yes_params),
                no: (no_id, no_params),
                ..
            }) => {
                self.emit_branch_cmp_imm(*op, *lhs, *imm, *yes_id, *yes_params, *no_id, *no_params)?
            }
            Some(ir::Terminator::Jump { id, params, .. }) => self.emit_jump(*id, *params)?,
            Some(ir::Terminator::Tail {
                func: tail_func,
                args,
                ..
            }) if *tail_func == self.func.id => self.emit_self_tail(args)?,
            Some(_) => skip!(self.func, "unsupported terminator"),
        }
        Some(())
    }

    /// Store the optional return value back to `vm.r[0]` and return to Rust.
    fn emit_return(&mut self, value: Option<ir::Id>) -> Option<()> {
        if let Some(value) = value {
            let Some(r) = reg_of(self.regs, value) else {
                skip!(self.func, "return value %v{} unallocated", value.0);
            };
            emit(self.out, Insn::StoreSlot { src: r, slot: 0 });
        }
        emit(self.out, Insn::Ret);
        Some(())
    }

    /// Emit a boolean branch where the condition is already materialized.
    fn emit_branch(
        &mut self,
        cond: ir::Id,
        yes_id: ir::Id,
        yes_params: ir::ParamsId,
        no_id: ir::Id,
        no_params: ir::ParamsId,
    ) -> Option<()> {
        let Some(yes_dst) = branch_target_params(self.func, yes_id) else {
            skip!(self.func, "bad branch target b{}", yes_id.0);
        };
        let yes_src = self.func.params(yes_params);
        // The current layout assumes each edge owns its parameter moves. Emit
        // the yes-edge shuffle before the conditional jump to the yes block.
        self.emit_edge_params(yes_src, yes_dst)?;

        let Some(cond) = reg_of(self.regs, cond) else {
            skip!(self.func, "unallocated branch condition");
        };
        emit(
            self.out,
            Insn::Test {
                lhs: cond,
                rhs: cond,
            },
        );
        self.defer_jnz(yes_id);

        let Some(no_dst) = branch_target_params(self.func, no_id) else {
            skip!(self.func, "bad branch target b{}", no_id.0);
        };
        let no_src = self.func.params(no_params);
        // The no edge is laid out after the conditional jump, so its shuffle
        // happens on fallthrough and then branches to the no block.
        self.emit_edge_params(no_src, no_dst)?;
        self.defer_jmp(no_id);
        Some(())
    }

    /// Emit a compare-immediate branch without materializing a boolean.
    fn emit_branch_cmp_imm(
        &mut self,
        op: BinOp,
        lhs: ir::Id,
        imm: i32,
        yes_id: ir::Id,
        yes_params: ir::ParamsId,
        no_id: ir::Id,
        no_params: ir::ParamsId,
    ) -> Option<()> {
        let Some(yes_dst) = branch_target_params(self.func, yes_id) else {
            skip!(self.func, "bad branch target b{}", yes_id.0);
        };
        let Some(no_dst) = branch_target_params(self.func, no_id) else {
            skip!(self.func, "bad branch target b{}", no_id.0);
        };
        let yes_src = self.func.params(yes_params);
        let no_src = self.func.params(no_params);

        self.emit_edge_params(yes_src, yes_dst)?;
        let Some(lhs) = reg_of(self.regs, lhs) else {
            skip!(self.func, "unallocated branch comparison operand");
        };
        self.emit_cmp_imm(op, lhs, imm)?;

        if op == BinOp::IEq {
            // General case: branch to yes when equal; otherwise resolve the no
            // edge parameters in-place and jump to the no block.
            self.defer_jz(yes_id);
            self.emit_edge_params(no_src, no_dst)?;
            self.defer_jmp(no_id);
            return Some(());
        }

        skip!(self.func, "unsupported branch comparison op {op:?}");
    }

    /// Set x86 flags for a supported immediate comparison.
    fn emit_cmp_imm(&mut self, op: BinOp, lhs: u8, imm: i32) -> Option<()> {
        match op {
            BinOp::IEq if imm == 0 => emit(self.out, Insn::Test { lhs, rhs: lhs }),
            BinOp::IEq => emit(self.out, Insn::CmpImm { reg: lhs, imm }),
            _ => skip!(self.func, "unsupported branch comparison op {op:?}"),
        }
        Some(())
    }

    /// Emit an unconditional IR jump after resolving edge parameters.
    fn emit_jump(&mut self, id: ir::Id, params: ir::ParamsId) -> Option<()> {
        let Some(dst_params) = branch_target_params(self.func, id) else {
            skip!(self.func, "bad jump target b{}", id.0);
        };
        let src_params = self.func.params(params);
        self.emit_edge_params(src_params, dst_params)?;
        self.defer_jmp(id);
        Some(())
    }

    /// Lower a self tail-call as edge-parameter moves plus a jump to entry.
    fn emit_self_tail(&mut self, args: &[ir::Id]) -> Option<()> {
        self.emit_edge_params(args, &self.func.params)?;
        self.defer_jmp(self.entry);
        Some(())
    }

    /// Resolve IR edge parameters, which are phi nodes on the predecessor edge.
    fn emit_edge_params(&mut self, src_params: &[ir::Id], dst_params: &[ir::Id]) -> Option<()> {
        // Edge params are the IR's phi nodes. They must be resolved at the
        // predecessor edge, before control reaches the target block.
        if !emit_param_moves(
            self.out,
            self.regs,
            &mut self.move_pairs,
            self.scratch,
            src_params,
            dst_params,
        ) {
            skip!(self.func, "could not resolve edge-param shuffle");
        }
        Some(())
    }

    /// Emit an unconditional jump placeholder to be patched after all blocks.
    fn defer_jmp(&mut self, target: ir::Id) {
        let rel = emit_jmp_placeholder(self.out);
        self.patches.push(Patch { rel, target });
    }

    /// Emit a `jnz` placeholder to be patched after all blocks.
    fn defer_jnz(&mut self, target: ir::Id) {
        let rel = emit_jnz_placeholder(self.out);
        self.patches.push(Patch { rel, target });
    }

    /// Emit a `jz` placeholder to be patched after all blocks.
    fn defer_jz(&mut self, target: ir::Id) {
        let rel = emit_jz_placeholder(self.out);
        self.patches.push(Patch { rel, target });
    }

    /// Patch every deferred branch once block offsets are known.
    fn patch_jumps(self) -> Option<()> {
        for Patch { rel, target } in self.patches {
            let Some(target) = self
                .block_offsets
                .get(target.0 as usize)
                .copied()
                .filter(|offset| *offset != usize::MAX)
            else {
                skip!(self.func, "bad patch target b{}", target.0);
            };
            patch_rel32(self.out, rel, target)?;
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::Insn;

    /// Encode one instruction into a fresh byte buffer.
    fn enc(insn: Insn) -> Vec<u8> {
        let mut code = Vec::new();
        insn.encode(&mut code);
        code
    }

    #[test]
    /// Check the hand-written encoders for representative register and slot forms.
    fn slot_and_reg_encodings() {
        assert_eq!(
            enc(Insn::LoadSlot { dst: 0, slot: 0 }),
            [0x48, 0x8b, 0x47, 0x00]
        ); // mov rax,[rdi+0]
        assert_eq!(
            enc(Insn::StoreSlot { src: 1, slot: 1 }),
            [0x48, 0x89, 0x4f, 0x08]
        ); // mov [rdi+8],rcx
        assert_eq!(
            enc(Insn::LoadSlot { dst: 8, slot: 2 }),
            [0x4c, 0x8b, 0x47, 0x10]
        ); // mov r8,[rdi+16]
        assert_eq!(
            enc(Insn::LoadMem {
                dst: 0,
                base: 1,
                offset: 8
            }),
            [0x48, 0x8b, 0x41, 0x08]
        ); // mov rax,[rcx+8]
        assert_eq!(
            enc(Insn::StoreMem {
                base: 1,
                offset: 8,
                src: 2
            }),
            [0x48, 0x89, 0x51, 0x08]
        ); // mov [rcx+8],rdx
        assert_eq!(
            enc(Insn::LeaMem {
                dst: 0,
                base: 1,
                offset: 8
            }),
            [0x48, 0x8d, 0x41, 0x08]
        ); // lea rax,[rcx+8]
        assert_eq!(
            enc(Insn::LoadMem {
                dst: 8,
                base: 12,
                offset: 0
            }),
            [0x4d, 0x8b, 0x44, 0x24, 0x00]
        ); // mov r8,[r12+0]
        assert_eq!(
            enc(Insn::LoadMem {
                dst: 0,
                base: 1,
                offset: 128
            }),
            [0x48, 0x8b, 0x81, 0x80, 0x00, 0x00, 0x00]
        ); // mov rax,[rcx+128]
        assert_eq!(enc(Insn::Mov { dst: 0, src: 1 }), [0x48, 0x89, 0xc8]); // mov rax,rcx
        assert_eq!(enc(Insn::Add { dst: 0, src: 2 }), [0x48, 0x01, 0xd0]); // add rax,rdx
        assert_eq!(enc(Insn::Sub { dst: 0, src: 2 }), [0x48, 0x29, 0xd0]); // sub rax,rdx
        assert_eq!(enc(Insn::Cmp { lhs: 0, rhs: 2 }), [0x48, 0x39, 0xd0]); // cmp rax,rdx
        assert_eq!(enc(Insn::Imul { dst: 0, src: 2 }), [0x48, 0x0f, 0xaf, 0xc2]); // imul rax,rdx
        assert_eq!(
            enc(Insn::SubImm { dst: 0, imm: 1 }),
            [0x48, 0x81, 0xe8, 1, 0, 0, 0]
        ); // sub rax,1
        assert_eq!(enc(Insn::Test { lhs: 0, rhs: 0 }), [0x48, 0x85, 0xc0]); // test rax,rax
        assert_eq!(enc(Insn::Ret), [0xc3]);
    }
}
