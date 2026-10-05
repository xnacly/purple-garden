//! x86-64 JIT backend
//!
//! IR is lowered to native code in a single pass, uses xralloc2 to map SSA values to x86 GPRs while emitting.
//!
//! The native ABI passes `*mut Vm` in `rdi`. `Vm::r` being the first field, `rdi` is the base of the VM
//! register file. Arguments arrive in `vm.r[0..n]`; return register is `vm.r[0]`. All
//! computation happen in GPRs. The jit moves arguments from `vm.r[0..n]` to GPRs in a jitted
//! functions prologue.

use std::fmt;

use crate::regalloc::Xralloc2;
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

const RAX: u8 = 0;
const RCX: u8 = 1;
const RDX: u8 = 2;
const RBX: u8 = 3;
/// stack pointer; only touched to save registers and align calls.
const RSP: u8 = 4;
const RSI: u8 = 6;
/// rdi holds `*mut Vm` == `&vm.r[0]`, base for slot loads/stores.
const RDI: u8 = 7;
const R8: u8 = 8;
const R9: u8 = 9;
const R10: u8 = 10;
const R11: u8 = 11;
const R12: u8 = 12;
const R13: u8 = 13;
const R14: u8 = 14;
const R15: u8 = 15;

/// Allocatable GPRs, popped from the back: caller-saved first so leaf functions dont require a
/// prologue, `rcx`/`rdx` last since `idiv` clobbers. `rax` is a fixed scratch for
/// edge-move cycles, helper addresses and `idiv`.
const POOL: &[u8] = &[RBX, R12, R13, R14, R15, RDX, RCX, R11, R10, R9, R8, RSI];
const CALLEE_SAVED: &[u8] = &[RBX, R12, R13, R14, R15];
/// Allocatable registers a SysV helper call clobbers.
const CALLER_SAVED: &[u8] = &[RCX, RDX, RSI, R8, R9, R10, R11];

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
        emit(out, Insn::Ret);
        purple_garden_shared::trace!("[jit::x86] compiled {} ({} bytes)", func.name, out.len());
        return Some(());
    }

    ra.reset(liveness.len(), POOL);
    let calls = Lowering::new(func, liveness, ra, buffers, entry).emit()?;

    // Pushes realign rsp from 8 (mod 16) at entry; helper calls need it at 0.
    let saved = CALLEE_SAVED.iter().copied().filter(|&reg| ra.used(reg));
    let pad = calls && saved.clone().count() % 2 == 0;
    if !pad && saved.clone().next().is_none() {
        // The epilogue is a bare `ret`: return in place instead of jumping to it.
        for &rel in &buffers.to_epilogue {
            buffers.body[rel - 1] = 0xc3;
            buffers.body[rel..rel + 4].fill(0xcc);
        }
    }
    out.reserve(buffers.body.len() + 32);
    for reg in saved.clone() {
        emit(out, Insn::Push { reg });
    }
    if pad {
        emit(out, Insn::SubImm { dst: RSP, imm: 8 });
    }
    out.extend_from_slice(&buffers.body);
    if pad {
        emit(out, Insn::AddImm { dst: RSP, imm: 8 });
    }
    for reg in saved.rev() {
        emit(out, Insn::Pop { reg });
    }
    emit(out, Insn::Ret);

    purple_garden_shared::trace!("[jit::x86] compiled {} ({} bytes)", func.name, out.len());
    Some(())
}

fn is_result_slot_identity(func: &ir::Func<'_>, entry: ir::Id) -> bool {
    let Some(&result_param) = func.params.first() else {
        return false;
    };
    let Some(block) = func.blocks.get(entry.0 as usize) else {
        return false;
    };

    !block.tombstone
        && block.instructions.is_empty()
        && matches!(
            block.term,
            Some(ir::Terminator::Return {
                value: Some(value),
                ..
            }) if value == result_param
        )
}

#[derive(Debug, Clone, Copy)]
struct Patch {
    /// Offset of the 4-byte relative displacement inside `out`.
    rel: usize,
    /// IR block id matching the final machine-code offset
    target: ir::Id,
}

/// Reusable lowering allocation storage
#[derive(Debug, Default, Clone)]
pub struct Scratch {
    body: Vec<u8>,
    block_offsets: Vec<usize>,
    patches: Vec<Patch>,
    /// rel32 offsets of jumps to the shared epilogue.
    to_epilogue: Vec<usize>,
    move_pairs: Vec<(u8, u8)>,
    /// `(reg, copy)` pairs preserved across a clobbering instruction.
    saves: Vec<(u8, u8)>,
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
    /// `push r{reg}` / `pop r{reg}` (callee-save frame management).
    Push {
        reg: u8,
    },
    Pop {
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
            Insn::Push { reg } => {
                if reg >= 8 {
                    code.push(0x41);
                }
                code.push(0x50 + (reg & 7));
            }
            Insn::Pop { reg } => {
                if reg >= 8 {
                    code.push(0x41);
                }
                code.push(0x58 + (reg & 7));
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
            Insn::Push { reg } => write!(f, "push {}", r(reg)),
            Insn::Pop { reg } => write!(f, "pop {}", r(reg)),
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

/// C-ABI `call addr` argument, rdi already holding `*mut Vm`.
#[derive(Clone, Copy)]
enum AbiArg {
    Reg(u8),
    Imm(u64),
}

/// Emit a small SysV ABI call. The VM base in `rdi` is saved because it is
/// caller-saved, while the generated code continues using it after the call.
fn emit_abi_call(
    out: &mut Vec<u8>,
    addr: u64,
    args: &[AbiArg],
    result: Option<u8>,
    stack_bytes: i32,
) {
    emit(
        out,
        Insn::SubImm {
            dst: RSP,
            imm: stack_bytes,
        },
    );
    emit(
        out,
        Insn::StoreMem {
            base: RSP,
            offset: 0,
            src: RDI,
        },
    );

    let abi_regs = [RDI, RSI, RDX, RCX, R8, R9];
    for (i, arg) in args.iter().enumerate() {
        let Some(&dst) = abi_regs.get(i) else { return };
        match *arg {
            AbiArg::Reg(src) if src != dst => emit(out, Insn::Mov { dst, src }),
            AbiArg::Imm(value) => emit(out, Insn::MovAbs { dst, imm: value }),
            AbiArg::Reg(_) => {}
        }
    }
    emit(out, Insn::MovAbs { dst: 0, imm: addr }); // rax = addr
    emit(out, Insn::CallReg { reg: 0 }); // call rax
    emit(
        out,
        Insn::LoadMem {
            dst: RDI,
            base: RSP,
            offset: 0,
        },
    );
    emit(
        out,
        Insn::AddImm {
            dst: RSP,
            imm: stack_bytes,
        },
    );
    if let Some(dst) = result {
        if dst != RAX {
            emit(out, Insn::Mov { dst, src: RAX });
        }
    }
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

/// Emit a register shuffle, resolving cycles through `scratch`.
fn emit_parallel_moves(out: &mut Vec<u8>, pairs: &mut Vec<(u8, u8)>, scratch: u8) {
    pairs.retain(|(src, dst)| src != dst);

    'outer: loop {
        if pairs.is_empty() {
            return;
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
        // Break one cycle by saving its head in the scratch register, which
        // never holds a value, then walk backwards through the freed dsts.

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

/// Return the parameter ids owned by a branch target block.
fn branch_target_params<'f>(func: &'f ir::Func<'_>, target: ir::Id) -> Option<&'f [ir::Id]> {
    func.blocks
        .get(target.0 as usize)
        .map(|block| func.params(block.params))
}

/// Whether a constant can be materialized by the current x86 lowering.
fn supported_const(value: &ir::Const<'_>) -> Option<i32> {
    match value {
        ir::Const::False => Some(0),
        ir::Const::True => Some(1),
        ir::Const::Int(i) if (*i as i32) < i32::MAX && (*i as i32) > i32::MIN => Some(*i as i32),
        _ => None,
    }
}

/// Bytes `emit_abi_call` reserves below rsp: the saved VM base, padded so rsp
/// stays 16-byte aligned. The prologue aligns rsp for functions with calls.
const ABI_CALL_STACK: i32 = 16;

#[derive(Clone, Copy)]
enum Divisor {
    Reg(u8),
    Imm(i32),
}

/// Per-function x86 lowering state, allocating registers as it emits.
///
/// `pos` steps in lockstep with [`ir::Func::live_set_into`]: two units per
/// block header, instruction and terminator; uses sit on `pos`, defs on
/// `pos + 1`.
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
    to_epilogue: &'a mut Vec<usize>,
    move_pairs: &'a mut Vec<(u8, u8)>,
    saves: &'a mut Vec<(u8, u8)>,
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
            to_epilogue,
            move_pairs,
            saves,
        } = buffers;
        body.clear();
        block_offsets.clear();
        block_offsets.resize(func.blocks.len(), usize::MAX);
        patches.clear();
        to_epilogue.clear();
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
            to_epilogue,
            move_pairs,
            saves,
        }
    }

    /// Emit the body, ending where the epilogue is appended. Returns whether
    /// it calls helpers.
    fn emit(mut self) -> Option<bool> {
        for (slot, &param) in self.func.params.iter().enumerate() {
            if self
                .liveness
                .get(param.0 as usize)
                .is_some_and(|l| l.0 != u32::MAX)
            {
                let dst = self.def(param, 0)?;
                emit(
                    self.out,
                    Insn::LoadSlot {
                        dst,
                        slot: slot as u8,
                    },
                );
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

    /// Register of `id`, assigned at `at` if this is its first touch.
    fn def(&mut self, id: ir::Id, at: u32) -> Option<u8> {
        let Some(&(start, last_use)) = self.liveness.get(id.0 as usize) else {
            skip!(self.func, "no liveness for %v{}", id.0);
        };
        let Some(reg) = self.ra.alloc(id, at.min(start), last_use) else {
            skip!(self.func, "out of registers at %v{}", id.0);
        };
        Some(reg)
    }

    fn reg(&self, id: ir::Id) -> Option<u8> {
        let Some(reg) = self.ra.reg(id) else {
            skip!(self.func, "use of unassigned %v{}", id.0);
        };
        Some(reg)
    }

    /// Run `body` with every register in `clobbers` free to overwrite: values
    /// living in them past this instruction are copied out and back. Copies
    /// are taken before `dst` is assigned so neither the copies nor `dst`
    /// reuse an operand register `body` still reads. Locations stay fixed,
    /// which keeps every CFG edge agreeing on them.
    fn clobbering(
        &mut self,
        clobbers: &[u8],
        dst: Option<ir::Id>,
        body: impl FnOnce(&mut Self, Option<u8>),
    ) -> Option<()> {
        let mut saves = std::mem::take(self.saves);
        saves.clear();
        saves.extend(
            self.ra
                .live_after(self.pos)
                .filter(|reg| clobbers.contains(reg))
                .map(|reg| (reg, reg)),
        );
        for (_, copy) in &mut saves {
            let Some(reg) = self.ra.take_free(|reg| !clobbers.contains(&reg)) else {
                skip!(
                    self.func,
                    "no free register to preserve r{copy} across a clobber"
                );
            };
            *copy = reg;
        }
        let dst = match dst {
            Some(dst) => Some(self.def(dst, self.pos + 1)?),
            None => None,
        };

        for &(reg, copy) in &saves {
            emit(
                self.out,
                Insn::Mov {
                    dst: copy,
                    src: reg,
                },
            );
        }
        body(self, dst);
        for &(reg, copy) in &saves {
            emit(
                self.out,
                Insn::Mov {
                    dst: reg,
                    src: copy,
                },
            );
            self.ra.give_back(copy);
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
                            AbiArg::Reg(RDI),
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
                    skip!(
                        self.func,
                        "const not true, false or i32::MIN < i < i32::MAX"
                    );
                };
                let dst = self.def(dst.id, self.pos + 1)?;
                emit(self.out, Insn::MovImm { dst, imm });
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
                emit(
                    self.out,
                    Insn::StoreMem {
                        base,
                        offset: *offset,
                        src,
                    },
                );
            }
            ir::Instr::Load {
                dst, base, offset, ..
            } if *offset <= i32::MAX as u32 => {
                let base = self.reg(*base)?;
                let dst = self.def(dst.id, self.pos + 1)?;
                emit(
                    self.out,
                    Insn::LoadMem {
                        dst,
                        base,
                        offset: *offset,
                    },
                );
            }
            ir::Instr::AddrOf {
                dst, base, offset, ..
            } if *offset <= i32::MAX as u32 => {
                let base = self.reg(*base)?;
                let dst = self.def(dst.id, self.pos + 1)?;
                emit(
                    self.out,
                    Insn::LeaMem {
                        dst,
                        base,
                        offset: *offset,
                    },
                );
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
            BinOp::IEq => {
                let d = self.def(dst, self.pos + 1)?;
                self.emit_cmp_imm(l, imm);
                self.emit_sete(d);
            }
            BinOp::IDiv | BinOp::IMod if imm == 0 => {
                self.def(dst, self.pos + 1)?;
                self.trap_div_zero();
            }
            BinOp::IMod if imm == 2 => {
                let d = self.def(dst, self.pos + 1)?;
                if d != l {
                    emit(self.out, Insn::Mov { dst: d, src: l });
                }
                emit(self.out, Insn::AndImm { dst: d, imm: 1 });
            }
            BinOp::IDiv | BinOp::IMod => self.emit_div(op, dst, l, Divisor::Imm(imm))?,
            _ => skip!(self.func, "unsupported binimm op {op:?}"),
        }
        Some(())
    }

    fn emit_bin(&mut self, op: BinOp, dst: ir::Id, lhs: ir::Id, rhs: ir::Id) -> Option<()> {
        let (l, r) = (self.reg(lhs)?, self.reg(rhs)?);
        match op {
            BinOp::IAdd | BinOp::ISub | BinOp::IMul => {
                let d = self.def(dst, self.pos + 1)?;
                emit_bin(self.out, op, d, l, r);
            }
            BinOp::IEq => {
                let d = self.def(dst, self.pos + 1)?;
                emit(self.out, Insn::Cmp { lhs: l, rhs: r });
                self.emit_sete(d);
            }
            BinOp::IDiv | BinOp::IMod => {
                // A register divisor can't be checked at compile time.
                emit(self.out, Insn::Test { lhs: r, rhs: r });
                let nonzero = emit_jnz_placeholder(self.out);
                self.trap_div_zero();
                let resume = self.out.len();
                patch_rel32(self.out, nonzero, resume)?;
                self.emit_div(op, dst, l, Divisor::Reg(r))?;
            }
            _ => skip!(self.func, "unsupported bin op {op:?}"),
        }
        Some(())
    }

    /// `idiv` divides `rdx:rax`, so the dividend goes through the scratch
    /// `rax` and `rdx` is clobbered by `cqo`. A divisor without a usable
    /// register (an immediate, or one living in `rdx`) is staged in `rcx`.
    ///
    /// `i64::MIN % -1` faults in `idiv` (#DE); the bytecode VM panics on the
    /// same input via Rust's checked `%`, so neither path is lenient here.
    fn emit_div(&mut self, op: BinOp, dst: ir::Id, l: u8, divisor: Divisor) -> Option<()> {
        let staged = !matches!(divisor, Divisor::Reg(r) if r != RDX);
        let clobbers: &[u8] = if staged { &[RCX, RDX] } else { &[RDX] };
        self.clobbering(clobbers, Some(dst), |this, d| {
            emit(this.out, Insn::Mov { dst: RAX, src: l });
            let divisor = match divisor {
                Divisor::Reg(r) if r != RDX => r,
                Divisor::Reg(r) => {
                    emit(this.out, Insn::Mov { dst: RCX, src: r });
                    RCX
                }
                Divisor::Imm(imm) => {
                    emit(this.out, Insn::MovImm { dst: RCX, imm });
                    RCX
                }
            };
            emit(this.out, Insn::Cqo);
            emit(this.out, Insn::Idiv { divisor });
            let src = if matches!(op, BinOp::IDiv) { RAX } else { RDX };
            if let Some(d) = d {
                emit(this.out, Insn::Mov { dst: d, src });
            }
        })
    }

    /// Raise the trap and leave: nothing live needs to survive the call.
    fn trap_div_zero(&mut self) {
        let helper: purple_garden_runtime::BuiltinFn = purple_garden_runtime::jit_trap_div_zero;
        self.call(helper as usize as u64, &[AbiArg::Reg(RDI)], None);
        self.jmp_epilogue();
    }

    fn call(&mut self, addr: u64, args: &[AbiArg], result: Option<u8>) {
        self.calls = true;
        emit_abi_call(self.out, addr, args, result, ABI_CALL_STACK);
    }

    fn emit_cmp_imm(&mut self, lhs: u8, imm: i32) {
        if imm == 0 {
            emit(self.out, Insn::Test { lhs, rhs: lhs });
        } else {
            emit(self.out, Insn::CmpImm { reg: lhs, imm });
        }
    }

    /// Materialize the current equality flag into a full 64-bit boolean value.
    fn emit_sete(&mut self, dst: u8) {
        emit(self.out, Insn::MovImm { dst, imm: 0 });
        emit(self.out, Insn::Sete { dst });
    }

    fn emit_term(&mut self, term: Option<&ir::Terminator>) -> Option<()> {
        let pos = self.pos;
        match term {
            None => {}
            Some(ir::Terminator::Return { value, .. }) => {
                if let Some(value) = value {
                    let src = self.reg(*value)?;
                    emit(self.out, Insn::StoreSlot { src, slot: 0 });
                }
                self.jmp_epilogue();
            }
            Some(ir::Terminator::Branch { cond, yes, no, .. }) => {
                // Same phasing as liveness: yes moves, then cond, then no moves.
                self.edge(*yes, pos)?;
                let cond = self.reg(*cond)?;
                emit(
                    self.out,
                    Insn::Test {
                        lhs: cond,
                        rhs: cond,
                    },
                );
                self.defer(emit_jnz_placeholder, yes.0);
                self.edge(*no, pos + 1)?;
                self.defer(emit_jmp_placeholder, no.0);
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
                self.emit_cmp_imm(lhs, *imm);
                self.defer(emit_jz_placeholder, yes.0);
                self.edge(*no, pos + 1)?;
                self.defer(emit_jmp_placeholder, no.0);
            }
            Some(ir::Terminator::Jump { id, params, .. }) => {
                self.edge((*id, *params), pos)?;
                self.defer(emit_jmp_placeholder, *id);
            }
            Some(ir::Terminator::Tail { func, args, .. }) if *func == self.func.id => {
                self.moves(args, &self.func.params, pos)?;
                self.defer(emit_jmp_placeholder, self.entry);
            }
            Some(_) => skip!(self.func, "unsupported terminator"),
        }
        Some(())
    }

    /// Resolve the edge's block params, the IR's phi nodes, on the predecessor.
    fn edge(&mut self, (target, params): (ir::Id, ir::ParamsId), at: u32) -> Option<()> {
        let Some(dst) = branch_target_params(self.func, target) else {
            skip!(self.func, "bad branch target b{}", target.0);
        };
        self.moves(self.func.params(params), dst, at)
    }

    /// Parallel move `src -> dst`. A forward edge reaches its target's params
    /// before the target block does, so they are assigned here; the register
    /// is then reserved from this edge on, keeping the move from landing on a
    /// value still live in between.
    fn moves(&mut self, src: &[ir::Id], dst: &[ir::Id], at: u32) -> Option<()> {
        if src.len() != dst.len() {
            skip!(self.func, "edge arity mismatch");
        }
        let mut pairs = std::mem::take(self.move_pairs);
        pairs.clear();
        for (&s, &d) in src.iter().zip(dst) {
            let s = self.reg(s)?;
            pairs.push((s, self.def(d, at)?));
        }
        emit_parallel_moves(self.out, &mut pairs, RAX);
        *self.move_pairs = pairs;
        Some(())
    }

    fn defer(&mut self, placeholder: fn(&mut Vec<u8>) -> usize, target: ir::Id) {
        let rel = placeholder(self.out);
        self.patches.push(Patch { rel, target });
    }

    fn jmp_epilogue(&mut self) {
        let rel = emit_jmp_placeholder(self.out);
        self.to_epilogue.push(rel);
    }

    /// Patch block jumps, then epilogue jumps to the end of the body. A jump
    /// ending the body falls through into the epilogue and is dropped.
    fn patch_jumps(&mut self) -> Option<()> {
        if self
            .to_epilogue
            .last()
            .is_some_and(|&rel| rel + 4 == self.out.len())
        {
            self.to_epilogue.pop();
            self.out.truncate(self.out.len() - 5);
        }
        let end = self.out.len();
        for Patch { rel, target } in self.patches.drain(..) {
            let Some(target) = self
                .block_offsets
                .get(target.0 as usize)
                .copied()
                .filter(|offset| *offset != usize::MAX)
            else {
                skip!(self.func, "bad patch target b{}", target.0);
            };
            patch_rel32(self.out, rel, target.min(end))?;
        }
        for &rel in self.to_epilogue.iter() {
            patch_rel32(self.out, rel, end)?;
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
