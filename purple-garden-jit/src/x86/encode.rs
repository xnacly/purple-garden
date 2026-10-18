//! x86-64 encoding for the instruction subset the lowering emits.
//!
//! Register numbers are x86's physical GPR numbering:
//!
//! ```text
//! 0 rax   1 rcx   2 rdx   3 rbx   4 rsp   5 rbp   6 rsi   7 rdi
//! 8 r8    9 r9   10 r10  11 r11  12 r12  13 r13  14 r14  15 r15
//! ```
//!
//! The low three bits go into ModRM/SIB fields. Bit 3 is carried by a REX
//! prefix (`REX.R` for the ModRM `reg` field, `REX.B` for the ModRM `r/m`
//! field, or opcode low bits for `movabs`).
//!
//! The SSE registers `xmm0..xmm15` are a separate file with the same
//! numbering and the same ModRM/REX split; the opcode's mandatory prefix
//! (`f2` for scalar double ops) is what makes a register number mean an xmm.

use std::{alloc::Allocator, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reg(pub u8);

pub const RAX: Reg = Reg(0);
pub const RCX: Reg = Reg(1);
pub const RDX: Reg = Reg(2);
pub const RBX: Reg = Reg(3);
pub const RSP: Reg = Reg(4);
pub const RBP: Reg = Reg(5);
pub const RSI: Reg = Reg(6);
pub const RDI: Reg = Reg(7);
pub const R8: Reg = Reg(8);
pub const R9: Reg = Reg(9);
pub const R10: Reg = Reg(10);
pub const R11: Reg = Reg(11);
pub const R12: Reg = Reg(12);
pub const R13: Reg = Reg(13);
pub const R14: Reg = Reg(14);
pub const R15: Reg = Reg(15);

impl fmt::Display for Reg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            [
                "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11",
                "r12", "r13", "r14", "r15",
            ][self.0 as usize],
        )
    }
}

/// An SSE register holding one double in its low 64 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Xmm(pub u8);

impl fmt::Display for Xmm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "xmm{}", self.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Cond {
    Always,
    Zero,
    NotZero,
    /// Unsigned `>=`, the carry flag is clear.
    AboveEq,
    /// Unsigned `<`, the carry flag is set.
    Below,
}

impl Cond {
    #[must_use]
    pub fn invert(self) -> Self {
        match self {
            Cond::Zero => Cond::NotZero,
            Cond::NotZero => Cond::Zero,
            Cond::AboveEq => Cond::Below,
            Cond::Below => Cond::AboveEq,
            Cond::Always => unreachable!("an unconditional jump has no inverse"),
        }
    }
}

/// Emit a near jump with a zero rel32, returning the displacement's offset
/// for [`patch_rel32`].
pub fn jump<A: Allocator>(code: &mut Vec<u8, A>, cond: Cond) -> usize {
    match cond {
        Cond::Always => code.push(0xe9),
        Cond::Zero => code.extend_from_slice(&[0x0f, 0x84]),
        Cond::NotZero => code.extend_from_slice(&[0x0f, 0x85]),
        Cond::AboveEq => code.extend_from_slice(&[0x0f, 0x83]),
        Cond::Below => code.extend_from_slice(&[0x0f, 0x82]),
    }
    let rel = code.len();
    code.extend_from_slice(&[0; 4]);
    rel
}

/// Point the rel32 at `rel` to `target`, as a distance from `from`. For a
/// jump that is the end of the instruction, `rel + 4`, as x86 counts
/// displacements from there.
pub fn patch_rel32(code: &mut [u8], rel: usize, from: usize, target: usize) -> Option<()> {
    let disp = i32::try_from(target as isize - from as isize).ok()?;
    code[rel..rel + 4].copy_from_slice(&disp.to_le_bytes());
    Some(())
}

/// A single x86-64 instruction. `encode` appends its machine-code bytes
#[derive(Debug, Clone, Copy)]
pub enum Insn {
    Ret,
    /// `mov r{dst}, [r{base} + offset]`
    LoadMem {
        dst: Reg,
        base: Reg,
        offset: u32,
    },
    /// `mov [r{base} + offset], r{src}`
    StoreMem {
        base: Reg,
        offset: u32,
        src: Reg,
    },
    /// `lea r{dst}, [r{base} + offset]`
    LeaMem {
        dst: Reg,
        base: Reg,
        offset: u32,
    },
    /// `mov r{dst}, r{src}`
    Mov {
        dst: Reg,
        src: Reg,
    },
    /// `mov r{dst}, imm` (sign-extended into 64 bits)
    MovImm {
        dst: Reg,
        imm: i32,
    },
    /// `add r{dst}, r{src}`
    Add {
        dst: Reg,
        src: Reg,
    },
    /// `sub r{dst}, r{src}`
    Sub {
        dst: Reg,
        src: Reg,
    },
    /// `imul r{dst}, r{src}`
    Imul {
        dst: Reg,
        src: Reg,
    },
    /// `imul r{dst}, r{src}, imm`
    ImulImm {
        dst: Reg,
        src: Reg,
        imm: i32,
    },
    /// `neg r{reg}` (two's-complement negate)
    Neg {
        reg: Reg,
    },
    /// `add r{dst}, imm`
    AddImm {
        dst: Reg,
        imm: i32,
    },
    /// `sub r{dst}, imm`
    SubImm {
        dst: Reg,
        imm: i32,
    },
    /// `and r{dst}, imm`
    AndImm {
        dst: Reg,
        imm: i32,
    },
    /// `cmp r{reg}, imm`
    CmpImm {
        reg: Reg,
        imm: i32,
    },
    /// `cmp r{lhs}, r{rhs}`
    Cmp {
        lhs: Reg,
        rhs: Reg,
    },
    /// `cmp r{reg}, [r{base} + offset]`
    CmpMem {
        reg: Reg,
        base: Reg,
        offset: u32,
    },
    /// `test r{lhs}, r{rhs}`
    Test {
        lhs: Reg,
        rhs: Reg,
    },
    /// `sete r{dst}b`; set r{dst}'s low byte to 1 if the last compare was equal.
    Sete {
        dst: Reg,
    },
    /// `setl r{dst}b`; signed less than.
    Setl {
        dst: Reg,
    },
    /// `setg r{dst}b`; signed greater than.
    Setg {
        dst: Reg,
    },
    /// `seta r{dst}b`; unsigned greater than, how [`Insn::Ucomisd`] reports `>`.
    Seta {
        dst: Reg,
    },
    /// `setne r{dst}b`; set r{dst}'s low byte to 1 if the last compare was not equal.
    Setne {
        dst: Reg,
    },
    /// `cmovae r{dst}, r{src}`; move if unsigned `>=`, the carry flag is clear.
    Cmovae {
        dst: Reg,
        src: Reg,
    },
    /// `cmovp r{dst}, r{src}`; move if the parity flag is set, which [`Insn::Ucomisd`] sets for
    /// NaN.
    Cmovp {
        dst: Reg,
        src: Reg,
    },
    /// `movabs r{dst}, imm64`; `MovImm` is i32-only, addresses need 64 bits.
    MovAbs {
        dst: Reg,
        imm: u64,
    },
    /// `call r{reg}`
    CallReg {
        reg: Reg,
    },
    /// `call rel32`, `disp` relative to the end of the instruction.
    CallRel {
        disp: i32,
    },
    /// `push r{reg}` / `pop r{reg}` (callee-save frame management).
    Push {
        reg: Reg,
    },
    Pop {
        reg: Reg,
    },
    /// `shr r{dst}, imm`; logical shift right.
    ShrImm {
        dst: Reg,
        imm: u8,
    },
    /// `sar r{dst}, imm`; arithmetic shift right, filling with the sign bit.
    SarImm {
        dst: Reg,
        imm: u8,
    },
    /// `lea r{dst}, [rip + disp]`; `disp` counts from the end of the
    /// instruction, so it patches like a jump's rel32.
    LeaRip {
        dst: Reg,
        disp: i32,
    },
    /// `movsxd r{dst}, dword [r{base} + r{index}*4]`; load a sign-extended i32
    /// table entry.
    LoadI32Scaled {
        dst: Reg,
        base: Reg,
        index: Reg,
    },
    /// `mov r{dst}, qword [r{base} + r{index}*8]`; load a value table entry.
    LoadScaled8 {
        dst: Reg,
        base: Reg,
        index: Reg,
    },
    /// `jmp r{reg}`
    JmpReg {
        reg: Reg,
    },
    /// `cqo`; sign-extend rax into rdx:rax (the idiv dividend).
    Cqo,
    /// `idiv r{divisor}`; rdx:rax / divisor, quotient to rax, remainder to rdx.
    Idiv {
        divisor: Reg,
    },
    /// `vaddsd xmm{dst}, xmm{lhs}, xmm{rhs}`: AVX's three-operand form, so dst
    /// may alias either operand. The high lane comes from lhs, not from dst,
    /// so unlike the SSE ops it doesn't depend on dst's old value. The same
    /// for sub, mul and div below.
    VAddSd {
        dst: Xmm,
        lhs: Xmm,
        rhs: Xmm,
    },
    VSubSd {
        dst: Xmm,
        lhs: Xmm,
        rhs: Xmm,
    },
    VMulSd {
        dst: Xmm,
        lhs: Xmm,
        rhs: Xmm,
    },
    VDivSd {
        dst: Xmm,
        lhs: Xmm,
        rhs: Xmm,
    },
    /// `ucomisd xmm{lhs}, xmm{rhs}`: compares the low doubles into the flags
    /// like an unsigned compare. Unordered (a NaN) sets ZF, PF and CF.
    Ucomisd {
        lhs: Xmm,
        rhs: Xmm,
    },
    /// `movaps xmm{dst}, xmm{src}`: copies the whole register, so unlike
    /// `movsd` it doesn't wait on dst's old value.
    Movaps {
        dst: Xmm,
        src: Xmm,
    },
    /// `xorps xmm{dst}, xmm{dst}`: zeroes dst without reading it, so ops that
    /// only write the low lane don't wait on its old value.
    Zeroxmm {
        dst: Xmm,
    },
    /// `movq xmm{dst}, r{src}`: the GPR's 64 bits become the low double,
    /// the high lane is zeroed. How a double constant reaches an xmm.
    MovqToXmm {
        dst: Xmm,
        src: Reg,
    },
    /// `movq r{dst}, xmm{src}`: the low double's bits into a GPR.
    MovqFromXmm {
        dst: Reg,
        src: Xmm,
    },
    /// `movsd xmm{dst}, [r{base} + offset]`
    LoadMemSd {
        dst: Xmm,
        base: Reg,
        offset: u32,
    },
    /// `movsd [r{base} + offset], xmm{src}`
    StoreMemSd {
        base: Reg,
        offset: u32,
        src: Xmm,
    },
    /// `cvtsi2sd xmm{dst}, r{src}`: the signed int as the low double, the high
    /// lane of dst is left alone.
    Cvtsi2sd {
        dst: Xmm,
        src: Reg,
    },
    /// `cvttsd2si r{dst}, xmm{src}`: the low double truncated toward zero, `i64::MIN` for NaN
    /// and out of range doubles.
    Cvttsd2si {
        dst: Reg,
        src: Xmm,
    },
}

impl Insn {
    /// Append this instruction's x86-64 machine-code bytes to `code`.
    pub fn encode<A: Allocator>(self, code: &mut Vec<u8, A>) {
        match self {
            Insn::Ret => code.push(0xc3),
            // Record memory ops all use the same ModRM/SIB memory form. The
            // opcode selects load, store, or address calculation; ModRM.reg is
            // the register operand for all three encodings.
            Insn::LoadMem { dst, base, offset } => mem_disp(code, 0x8b, dst.0, base.0, offset),
            Insn::StoreMem { base, offset, src } => mem_disp(code, 0x89, src.0, base.0, offset),
            Insn::LeaMem { dst, base, offset } => mem_disp(code, 0x8d, dst.0, base.0, offset),
            Insn::LoadMemSd { dst, base, offset } => movsd_mem(code, 0x10, dst.0, base.0, offset),
            Insn::StoreMemSd { base, offset, src } => movsd_mem(code, 0x11, src.0, base.0, offset),
            // 0x3b = `cmp r64, r/m64`.
            Insn::CmpMem { reg, base, offset } => mem_disp(code, 0x3b, reg.0, base.0, offset),
            // 0x89 = `mov r/m64, r64`.
            // ModRM.reg encodes src; ModRM.r/m encodes dst.
            Insn::Mov { dst, src } => reg_reg(code, 0x89, src.0, dst.0),
            // 0x01 = `add r/m64, r64`, 0x29 = `sub r/m64, r64`.
            // Same direction as mov: reg is src, r/m is dst.
            Insn::Add { dst, src } => reg_reg(code, 0x01, src.0, dst.0),
            Insn::Sub { dst, src } => reg_reg(code, 0x29, src.0, dst.0),
            // 0x0f 0xaf = `imul r64, r/m64`.
            // Here ModRM.reg is dst and ModRM.r/m is src, opposite of add/sub.
            Insn::Imul { dst, src } => {
                code.push(rex(dst.0, src.0));
                code.extend_from_slice(&[0x0f, 0xaf, modrm(dst.0, src.0)]);
            }
            // 0xf7 /3 = `neg r/m64`.
            // `/3` means ModRM.reg is not a register; it is the opcode extension
            // digit 3. ModRM.r/m names the operand.
            Insn::Neg { reg } => {
                code.push(rex(0, reg.0));
                code.extend_from_slice(&[0xf7, modrm(3, reg.0)]);
            }
            // 0x81 /digit = `op r/m64, imm32`.
            // /0 add, /4 and, /5 sub, /7 cmp.
            Insn::AddImm { dst, imm } => reg_imm(code, 0, dst.0, imm),
            Insn::SubImm { dst, imm } => reg_imm(code, 5, dst.0, imm),
            Insn::AndImm { dst, imm } => reg_imm(code, 4, dst.0, imm),
            Insn::CmpImm { reg, imm } => reg_imm(code, 7, reg.0, imm),
            Insn::Cmp { lhs, rhs } => reg_reg(code, 0x39, rhs.0, lhs.0),
            // 0x85 = `test r/m64, r64`.
            // Both operands are only read, but keep the same packing convention:
            // ModRM.reg = rhs, ModRM.r/m = lhs.
            Insn::Test { lhs, rhs } => reg_reg(code, 0x85, rhs.0, lhs.0),
            // 0xc7 /0 = `mov r/m64, imm32`.
            Insn::MovImm { dst, imm } => {
                code.push(rex(0, dst.0));
                code.push(0xc7);
                code.push(modrm(0, dst.0));
                code.extend_from_slice(&imm.to_le_bytes());
            }
            // 0x0f 0x94/0x9c/0x9f = `sete/setl/setg r/m8`.
            // The REX prefix is not REX.W here; it exists only so byte-register
            // names are the modern low-byte registers (`sil`, `dil`, `r8b`, ...).
            // ModRM.reg is /0, ModRM.r/m names the byte destination.
            Insn::Sete { dst } => setcc(code, 0x94, dst),
            Insn::Setl { dst } => setcc(code, 0x9c, dst),
            Insn::Setg { dst } => setcc(code, 0x9f, dst),
            Insn::Seta { dst } => setcc(code, 0x97, dst),
            Insn::Setne { dst } => setcc(code, 0x95, dst),
            // REX.W 0F 43/4A /r = `cmovae/cmovp r64, r/m64`, ModRM.reg is dst.
            Insn::Cmovae { dst, src } => {
                code.extend_from_slice(&[rex(dst.0, src.0), 0x0f, 0x43, modrm(dst.0, src.0)]);
            }
            Insn::Cmovp { dst, src } => {
                code.extend_from_slice(&[rex(dst.0, src.0), 0x0f, 0x4a, modrm(dst.0, src.0)]);
            }
            // REX.W 0x69 /r id = `imul r64, r/m64, imm32`, ModRM.reg is dst.
            Insn::ImulImm { dst, src, imm } => {
                code.extend_from_slice(&[rex(dst.0, src.0), 0x69, modrm(dst.0, src.0)]);
                code.extend_from_slice(&imm.to_le_bytes());
            }
            // REX.W 0xb8+rd io64 = `movabs r64, imm64`.
            // This form has no ModRM byte; the low 3 register bits are embedded
            // in the opcode and the high bit goes in REX.B.
            Insn::MovAbs { dst, imm } => {
                code.push(0x48 | u8::from(dst.0 >= 8));
                code.push(0xb8 + (dst.0 & 7));
                code.extend_from_slice(&imm.to_le_bytes());
            }
            // 0xff /2 = `call r/m64`.
            // `/2` is the opcode extension; ModRM.r/m names the call target.
            // No REX.W is required. REX.B is enough to reach r8..r15.
            Insn::CallReg { reg } => {
                if reg.0 >= 8 {
                    code.push(0x41);
                }
                code.extend_from_slice(&[0xff, modrm(2, reg.0)]);
            }
            // 0xe8 rel32 = `call rel32`, relative to the end of the instruction.
            Insn::CallRel { disp } => {
                code.push(0xe8);
                code.extend_from_slice(&disp.to_le_bytes());
            }
            Insn::Push { reg } => {
                if reg.0 >= 8 {
                    code.push(0x41);
                }
                code.push(0x50 + (reg.0 & 7));
            }
            Insn::Pop { reg } => {
                if reg.0 >= 8 {
                    code.push(0x41);
                }
                code.push(0x58 + (reg.0 & 7));
            }
            // REX.W 0xc1 /5 ib = `shr r/m64, imm8`.
            Insn::ShrImm { dst, imm } => {
                code.extend_from_slice(&[rex(0, dst.0), 0xc1, modrm(5, dst.0), imm]);
            }
            // REX.W 0xc1 /7 ib = `sar r/m64, imm8`.
            Insn::SarImm { dst, imm } => {
                code.extend_from_slice(&[rex(0, dst.0), 0xc1, modrm(7, dst.0), imm]);
            }
            // REX.W 0x8d /r = `lea r64, m`. ModRM mod=00 r/m=101 is the
            // rip-relative form, a disp32 follows.
            Insn::LeaRip { dst, disp } => {
                code.extend_from_slice(&[rex(dst.0, 0), 0x8d, ((dst.0 & 7) << 3) | 0b101]);
                code.extend_from_slice(&disp.to_le_bytes());
            }
            // REX.W 0x63 /r = `movsxd r64, r/m32`, see [`scaled`] for the SIB form.
            Insn::LoadI32Scaled { dst, base, index } => {
                scaled(code, 0x63, 0b10, dst, base, index);
            }
            // REX.W 0x8b /r = `mov r64, r/m64`, same SIB form with scale=11 (*8).
            Insn::LoadScaled8 { dst, base, index } => scaled(code, 0x8b, 0b11, dst, base, index),
            // 0xff /4 = `jmp r/m64`, the indirect sibling of `call r/m64`.
            Insn::JmpReg { reg } => {
                if reg.0 >= 8 {
                    code.push(0x41);
                }
                code.extend_from_slice(&[0xff, modrm(4, reg.0)]);
            }
            // REX.W 0x99 ; cqo.
            Insn::Cqo => code.extend_from_slice(&[0x48, 0x99]),
            // REX.W 0xf7 /7 = `idiv r/m64`.
            // `/7` is the opcode extension; ModRM.r/m names the divisor.
            Insn::Idiv { divisor } => {
                code.push(rex(0, divisor.0));
                code.extend_from_slice(&[0xf7, modrm(7, divisor.0)]);
            }
            // VEX.LIG.F2.0F 58/5C/59/5E /r = `vaddsd/vsubsd/vmulsd/vdivsd xmm1,
            // xmm2, xmm3/m64`, the same opcodes as the SSE forms.
            Insn::VAddSd { dst, lhs, rhs } => vex_sd(code, 0x58, dst.0, lhs.0, rhs.0),
            Insn::VSubSd { dst, lhs, rhs } => vex_sd(code, 0x5c, dst.0, lhs.0, rhs.0),
            Insn::VMulSd { dst, lhs, rhs } => vex_sd(code, 0x59, dst.0, lhs.0, rhs.0),
            Insn::VDivSd { dst, lhs, rhs } => vex_sd(code, 0x5e, dst.0, lhs.0, rhs.0),
            // 66 0F 2E /r = `ucomisd xmm1, xmm2/m64`, ModRM.reg is lhs.
            Insn::Ucomisd { lhs, rhs } => sse(code, 0x66, 0x2e, lhs.0, rhs.0),
            // 0F 28 /r = `movaps xmm1, xmm2/m128`, ModRM.reg is dst.
            Insn::Movaps { dst, src } => sse_ps(code, 0x28, dst.0, src.0),
            // 0F 57 /r = `xorps xmm1, xmm2/m128`.
            Insn::Zeroxmm { dst } => sse_ps(code, 0x57, dst.0, dst.0),
            // 66 REX.W 0F 6E /r = `movq xmm, r/m64`. REX.W is what makes it
            // a 64-bit move (without it the same opcode is `movd`), so this
            // doesn't go through [`sse`], which emits REX only for high regs.
            Insn::MovqToXmm { dst, src } => {
                code.extend_from_slice(&[0x66, rex(dst.0, src.0), 0x0f, 0x6e, modrm(dst.0, src.0)]);
            }
            // 66 REX.W 0F 7E /r = `movq r/m64, xmm`, ModRM.reg is the xmm src.
            Insn::MovqFromXmm { dst, src } => {
                code.extend_from_slice(&[0x66, rex(src.0, dst.0), 0x0f, 0x7e, modrm(src.0, dst.0)]);
            }
            // F2 REX.W 0F 2A /r = `cvtsi2sd xmm, r/m64`, REX.W for the 64-bit int.
            Insn::Cvtsi2sd { dst, src } => {
                code.extend_from_slice(&[0xf2, rex(dst.0, src.0), 0x0f, 0x2a, modrm(dst.0, src.0)]);
            }
            // F2 REX.W 0F 2C /r = `cvttsd2si r64, xmm/m64`, ModRM.reg is the GPR dst.
            Insn::Cvttsd2si { dst, src } => {
                code.extend_from_slice(&[0xf2, rex(dst.0, src.0), 0x0f, 0x2c, modrm(dst.0, src.0)]);
            }
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
fn reg_reg<A: Allocator>(code: &mut Vec<u8, A>, opcode: u8, reg: u8, rm: u8) {
    code.extend_from_slice(&[rex(reg, rm), opcode, modrm(reg, rm)]);
}

/// Register-register SSE op: `prefix [REX] 0F opcode ModRM(reg, rm)`.
///
/// The mandatory `prefix` (`f2` for scalar double, `66` for packed integer)
/// goes before REX, which here only carries the high register bits: SSE ops
/// ignore REX.W, so the prefix is dropped when neither register needs it.
/// For the two-xmm ops `reg` is dst and `rm` is src.
fn sse<A: Allocator>(code: &mut Vec<u8, A>, prefix: u8, opcode: u8, reg: u8, rm: u8) {
    code.push(prefix);
    sse_ps(code, opcode, reg, rm);
}

/// [`sse`] without a prefix: the packed single ops, whose full register forms
/// also serve doubles.
fn sse_ps<A: Allocator>(code: &mut Vec<u8, A>, opcode: u8, reg: u8, rm: u8) {
    if reg >= 8 || rm >= 8 {
        code.push(0x40 | (u8::from(reg >= 8) << 2) | u8::from(rm >= 8));
    }
    code.extend_from_slice(&[0x0f, opcode, modrm(reg, rm)]);
}

/// Three-operand scalar double AVX op: `VEX opcode ModRM(reg, rm)`, with the
/// second operand in `VEX.vvvv`.
///
/// VEX replaces both the `f2` prefix (`pp=11`) and REX, storing the high
/// register bits inverted. The two byte form `c5` only has room for `R`, so
/// an `rm >= 8` needs the three byte form `c4`, which adds `X`, `B` and the
/// opcode map (`00001` for `0f`):
///
/// ```text
/// c5 [R vvvv L pp]
/// c4 [R X B mmmmm] [W vvvv L pp]
/// ```
fn vex_sd<A: Allocator>(code: &mut Vec<u8, A>, opcode: u8, reg: u8, vvvv: u8, rm: u8) {
    let r = u8::from(reg < 8) << 7;
    let vvvv_pp = ((!vvvv & 0xf) << 3) | 0b11;
    if rm < 8 {
        code.extend_from_slice(&[0xc5, r | vvvv_pp]);
    } else {
        code.extend_from_slice(&[0xc4, r | 0x40 | 0x01, vvvv_pp]);
    }
    code.extend_from_slice(&[opcode, modrm(reg, rm)]);
}

/// Register-immediate op: `REX.W 0x81 /digit r/m64, imm32`.
///
/// The `/digit` is encoded in ModRM.reg and selects the operation:
/// `/0 add`, `/4 and`, `/5 sub`, `/7 cmp`. The actual destination register is
/// ModRM.r/m.
fn reg_imm<A: Allocator>(code: &mut Vec<u8, A>, digit: u8, rm: u8, imm: i32) {
    code.push(rex(0, rm));
    code.push(0x81);
    code.push(modrm(digit, rm));
    code.extend_from_slice(&imm.to_le_bytes());
}

/// Memory op using `[base + offset]`, where ModRM.reg is the register operand
/// and ModRM.r/m names the base. Always emits an explicit displacement (disp8
/// when possible, otherwise disp32), which avoids zero-offset special cases for
/// rbp/r13. rsp/r12 bases require a SIB byte even with no index.
///
/// This helper is intended for record payload access. IR offsets are already
/// byte offsets, unlike VM register slots, so callers pass the offset through
/// unchanged.
fn mem_disp<A: Allocator>(code: &mut Vec<u8, A>, opcode: u8, reg: u8, base: u8, offset: u32) {
    code.extend_from_slice(&[rex(reg, base), opcode]);
    mem_operand(code, reg, base, offset);
}

/// `movsd` between xmm `reg` and `[base + offset]` (opcode 0x10 load, 0x11 store). `F2 0F op`,
/// with a REX only for extended registers since W means nothing here.
fn movsd_mem<A: Allocator>(code: &mut Vec<u8, A>, opcode: u8, reg: u8, base: u8, offset: u32) {
    code.push(0xf2);
    if reg >= 8 || base >= 8 {
        code.push(rex(reg, base) & !0x08);
    }
    code.extend_from_slice(&[0x0f, opcode]);
    mem_operand(code, reg, base, offset);
}

/// ModRM, SIB and displacement of `[base + offset]` with `reg` in ModRM.reg.
fn mem_operand<A: Allocator>(code: &mut Vec<u8, A>, reg: u8, base: u8, offset: u32) {
    let disp8 = u8::try_from(offset)
        .ok()
        .filter(|offset| *offset <= i8::MAX as u8);
    let mode = if disp8.is_some() { 0x40 } else { 0x80 };
    // In ModRM, r/m=100 does not mean `rsp` directly; it means a SIB byte
    // follows. That is mandatory for rsp/r12 bases, even without an index.
    let rm = if needs_sib(base) { RSP.0 } else { base & 7 };
    let m = mode | ((reg & 7) << 3) | rm;

    code.push(m);
    if needs_sib(base) {
        code.push(sib_no_index(base));
    }
    if let Some(disp) = disp8 {
        code.push(disp);
    } else {
        code.extend_from_slice(&offset.to_le_bytes());
    }
}

/// `opcode r64, [base + index*(1 << scale)]`: ModRM r/m=100 means a SIB byte
/// follows. The index register's high bit is REX.X, which [`rex`] doesn't set.
/// A base with low bits 101 (rbp/r13) and mod=00 would mean "no base, disp32",
/// so those take mod=01 with a zero disp8.
fn scaled<A: Allocator>(
    code: &mut Vec<u8, A>,
    opcode: u8,
    scale: u8,
    dst: Reg,
    base: Reg,
    index: Reg,
) {
    let rex = rex(dst.0, base.0) | (u8::from(index.0 >= 8) << 1);
    let mode = if base.0 & 7 == 0b101 { 0x40 } else { 0x00 };
    let sib = (scale << 6) | ((index.0 & 7) << 3) | (base.0 & 7);
    code.extend_from_slice(&[rex, opcode, mode | ((dst.0 & 7) << 3) | 0b100, sib]);
    if mode == 0x40 {
        code.push(0);
    }
}

fn setcc<A: Allocator>(code: &mut Vec<u8, A>, opcode: u8, dst: Reg) {
    code.push(0x40 | u8::from(dst.0 >= 8));
    code.extend_from_slice(&[0x0f, opcode, modrm(0, dst.0)]);
}

fn needs_sib(base: u8) -> bool {
    base & 7 == RSP.0
}

fn sib_no_index(base: u8) -> u8 {
    // scale=0, index=100 (none), base=base low bits.
    0x20 | (base & 7)
}

#[cfg(test)]
mod tests {
    use super::{Insn, Reg};

    /// Encode one instruction into a fresh byte buffer.
    fn enc(insn: Insn) -> Vec<u8> {
        let mut code = Vec::new();
        insn.encode(&mut code);
        code
    }

    #[test]
    /// Scalar double ops, bytes checked against gas.
    fn sse_encodings() {
        use super::Xmm;

        assert_eq!(
            enc(Insn::Ucomisd {
                lhs: Xmm(0),
                rhs: Xmm(1)
            }),
            [0x66, 0x0f, 0x2e, 0xc1]
        ); // ucomisd xmm0,xmm1
        assert_eq!(
            enc(Insn::Ucomisd {
                lhs: Xmm(2),
                rhs: Xmm(9)
            }),
            [0x66, 0x41, 0x0f, 0x2e, 0xd1]
        ); // ucomisd xmm2,xmm9
        assert_eq!(
            enc(Insn::Ucomisd {
                lhs: Xmm(12),
                rhs: Xmm(3)
            }),
            [0x66, 0x44, 0x0f, 0x2e, 0xe3]
        ); // ucomisd xmm12,xmm3
    }

    #[test]
    /// AVX scalar double ops, bytes checked against gas. An rhs >= xmm8 needs
    /// the three byte VEX prefix.
    fn vex_encodings() {
        use super::Xmm;

        assert_eq!(
            enc(Insn::VAddSd {
                dst: Xmm(0),
                lhs: Xmm(1),
                rhs: Xmm(2)
            }),
            [0xc5, 0xf3, 0x58, 0xc2]
        ); // vaddsd xmm0,xmm1,xmm2
        assert_eq!(
            enc(Insn::VSubSd {
                dst: Xmm(1),
                lhs: Xmm(0),
                rhs: Xmm(15)
            }),
            [0xc4, 0xc1, 0x7b, 0x5c, 0xcf]
        ); // vsubsd xmm1,xmm0,xmm15
        assert_eq!(
            enc(Insn::VMulSd {
                dst: Xmm(9),
                lhs: Xmm(14),
                rhs: Xmm(3)
            }),
            [0xc5, 0x0b, 0x59, 0xcb]
        ); // vmulsd xmm9,xmm14,xmm3
        assert_eq!(
            enc(Insn::VDivSd {
                dst: Xmm(8),
                lhs: Xmm(7),
                rhs: Xmm(12)
            }),
            [0xc4, 0x41, 0x43, 0x5e, 0xc4]
        ); // vdivsd xmm8,xmm7,xmm12
        assert_eq!(
            enc(Insn::VAddSd {
                dst: Xmm(0),
                lhs: Xmm(0),
                rhs: Xmm(0)
            }),
            [0xc5, 0xfb, 0x58, 0xc0]
        ); // vaddsd xmm0,xmm0,xmm0
    }

    #[test]
    /// Double moves between xmm, from a GPR and to and from memory, bytes checked against gas.
    fn sse_move_encodings() {
        use super::{R9, R12, RAX, RBP, RBX, RCX, Xmm};

        assert_eq!(
            enc(Insn::LoadMemSd {
                dst: Xmm(0),
                base: RCX,
                offset: 8
            }),
            [0xf2, 0x0f, 0x10, 0x41, 0x08]
        ); // movsd xmm0,[rcx+8]
        assert_eq!(
            enc(Insn::StoreMemSd {
                base: RCX,
                offset: 8,
                src: Xmm(2)
            }),
            [0xf2, 0x0f, 0x11, 0x51, 0x08]
        ); // movsd [rcx+8],xmm2
        assert_eq!(
            enc(Insn::LoadMemSd {
                dst: Xmm(9),
                base: R12,
                offset: 0x100
            }),
            [0xf2, 0x45, 0x0f, 0x10, 0x8c, 0x24, 0x00, 0x01, 0x00, 0x00]
        ); // movsd xmm9,[r12+0x100]
        assert_eq!(
            enc(Insn::StoreMemSd {
                base: RBP,
                offset: 0,
                src: Xmm(15)
            }),
            [0xf2, 0x44, 0x0f, 0x11, 0x7d, 0x00]
        ); // movsd [rbp+0],xmm15

        assert_eq!(
            enc(Insn::MovqFromXmm {
                dst: RAX,
                src: Xmm(3)
            }),
            [0x66, 0x48, 0x0f, 0x7e, 0xd8]
        ); // movq rax,xmm3
        assert_eq!(
            enc(Insn::MovqFromXmm {
                dst: R12,
                src: Xmm(9)
            }),
            [0x66, 0x4d, 0x0f, 0x7e, 0xcc]
        ); // movq r12,xmm9
        assert_eq!(
            enc(Insn::Cvtsi2sd {
                dst: Xmm(0),
                src: RAX
            }),
            [0xf2, 0x48, 0x0f, 0x2a, 0xc0]
        ); // cvtsi2sd xmm0,rax
        assert_eq!(
            enc(Insn::Cvtsi2sd {
                dst: Xmm(3),
                src: R12
            }),
            [0xf2, 0x49, 0x0f, 0x2a, 0xdc]
        ); // cvtsi2sd xmm3,r12
        assert_eq!(
            enc(Insn::Cvtsi2sd {
                dst: Xmm(9),
                src: RBX
            }),
            [0xf2, 0x4c, 0x0f, 0x2a, 0xcb]
        ); // cvtsi2sd xmm9,rbx
        assert_eq!(
            enc(Insn::Cvttsd2si {
                dst: RBX,
                src: Xmm(0)
            }),
            [0xf2, 0x48, 0x0f, 0x2c, 0xd8]
        ); // cvttsd2si rbx,xmm0
        assert_eq!(
            enc(Insn::Cvttsd2si {
                dst: R12,
                src: Xmm(9)
            }),
            [0xf2, 0x4d, 0x0f, 0x2c, 0xe1]
        ); // cvttsd2si r12,xmm9
        assert_eq!(
            enc(Insn::Movaps {
                dst: Xmm(0),
                src: Xmm(1)
            }),
            [0x0f, 0x28, 0xc1]
        ); // movaps xmm0,xmm1
        assert_eq!(
            enc(Insn::Movaps {
                dst: Xmm(2),
                src: Xmm(9)
            }),
            [0x41, 0x0f, 0x28, 0xd1]
        ); // movaps xmm2,xmm9
        assert_eq!(
            enc(Insn::Movaps {
                dst: Xmm(12),
                src: Xmm(3)
            }),
            [0x44, 0x0f, 0x28, 0xe3]
        ); // movaps xmm12,xmm3
        assert_eq!(enc(Insn::Zeroxmm { dst: Xmm(0) }), [0x0f, 0x57, 0xc0]); // xorps xmm0,xmm0
        assert_eq!(
            enc(Insn::Zeroxmm { dst: Xmm(11) }),
            [0x45, 0x0f, 0x57, 0xdb]
        ); // xorps xmm11,xmm11
        assert_eq!(
            enc(Insn::MovqToXmm {
                dst: Xmm(3),
                src: RAX
            }),
            [0x66, 0x48, 0x0f, 0x6e, 0xd8]
        ); // movq xmm3,rax
        assert_eq!(
            enc(Insn::MovqToXmm {
                dst: Xmm(10),
                src: R12
            }),
            [0x66, 0x4d, 0x0f, 0x6e, 0xd4]
        ); // movq xmm10,r12
        assert_eq!(
            enc(Insn::MovqToXmm {
                dst: Xmm(1),
                src: R9
            }),
            [0x66, 0x49, 0x0f, 0x6e, 0xc9]
        ); // movq xmm1,r9
    }

    #[test]
    /// Signed compares and the immediate multiply, bytes checked against objdump.
    fn compare_and_imul_imm_encodings() {
        use super::{R8, R9, R10, R12, RAX, RBX, RCX, RSI};

        assert_eq!(
            enc(Insn::CmpMem {
                reg: super::RSP,
                base: super::RDI,
                offset: 0x40
            }),
            [0x48, 0x3b, 0x67, 0x40]
        ); // cmp rsp,[rdi+0x40]
        assert_eq!(
            enc(Insn::CmpMem {
                reg: super::RSP,
                base: super::RDI,
                offset: 0x280
            }),
            [0x48, 0x3b, 0xa7, 0x80, 0x02, 0x00, 0x00]
        ); // cmp rsp,[rdi+0x280]
        assert_eq!(enc(Insn::Setl { dst: RSI }), [0x40, 0x0f, 0x9c, 0xc6]); // setl sil
        assert_eq!(enc(Insn::Setg { dst: R8 }), [0x41, 0x0f, 0x9f, 0xc0]); // setg r8b
        assert_eq!(enc(Insn::Sete { dst: RSI }), [0x40, 0x0f, 0x94, 0xc6]); // sete sil
        assert_eq!(enc(Insn::Seta { dst: R8 }), [0x41, 0x0f, 0x97, 0xc0]); // seta r8b
        assert_eq!(enc(Insn::Setne { dst: RSI }), [0x40, 0x0f, 0x95, 0xc6]); // setne sil
        assert_eq!(enc(Insn::Setne { dst: R8 }), [0x41, 0x0f, 0x95, 0xc0]); // setne r8b
        assert_eq!(
            enc(Insn::Cmovae { dst: RBX, src: RAX }),
            [0x48, 0x0f, 0x43, 0xd8]
        ); // cmovae rbx,rax
        assert_eq!(
            enc(Insn::Cmovae { dst: R12, src: RAX }),
            [0x4c, 0x0f, 0x43, 0xe0]
        ); // cmovae r12,rax
        assert_eq!(
            enc(Insn::Cmovp { dst: RBX, src: RAX }),
            [0x48, 0x0f, 0x4a, 0xd8]
        ); // cmovp rbx,rax
        assert_eq!(
            enc(Insn::Cmovp { dst: R12, src: RAX }),
            [0x4c, 0x0f, 0x4a, 0xe0]
        ); // cmovp r12,rax
        assert_eq!(
            enc(Insn::ImulImm {
                dst: RAX,
                src: RCX,
                imm: 600
            }),
            [0x48, 0x69, 0xc1, 0x58, 0x02, 0x00, 0x00]
        ); // imul rax,rcx,600
        assert_eq!(
            enc(Insn::ImulImm {
                dst: R9,
                src: R10,
                imm: 3
            }),
            [0x4d, 0x69, 0xca, 0x03, 0x00, 0x00, 0x00]
        ); // imul r9,r10,3
    }

    #[test]
    /// The jump table forms of a switch, bytes checked against objdump.
    fn switch_encodings() {
        use super::{Cond, R9, R11, R12, R13, RAX, RBX, RCX, RSI, jump};

        assert_eq!(
            enc(Insn::ShrImm { dst: RAX, imm: 3 }),
            [0x48, 0xc1, 0xe8, 0x03]
        ); // shr rax,3
        assert_eq!(
            enc(Insn::ShrImm { dst: R9, imm: 3 }),
            [0x49, 0xc1, 0xe9, 0x03]
        ); // shr r9,3
        assert_eq!(
            enc(Insn::SarImm { dst: RAX, imm: 63 }),
            [0x48, 0xc1, 0xf8, 0x3f]
        ); // sar rax,63
        assert_eq!(
            enc(Insn::SarImm { dst: R12, imm: 4 }),
            [0x49, 0xc1, 0xfc, 0x04]
        ); // sar r12,4
        assert_eq!(
            enc(Insn::LeaRip {
                dst: RCX,
                disp: 0x10
            }),
            [0x48, 0x8d, 0x0d, 0x10, 0x00, 0x00, 0x00]
        ); // lea rcx,[rip+0x10]
        assert_eq!(
            enc(Insn::LeaRip {
                dst: R13,
                disp: 0x10
            }),
            [0x4c, 0x8d, 0x2d, 0x10, 0x00, 0x00, 0x00]
        ); // lea r13,[rip+0x10]
        assert_eq!(
            enc(Insn::LoadI32Scaled {
                dst: RAX,
                base: RCX,
                index: RAX
            }),
            [0x48, 0x63, 0x04, 0x81]
        ); // movsxd rax,[rcx+rax*4]
        assert_eq!(
            enc(Insn::LoadI32Scaled {
                dst: RAX,
                base: RBX,
                index: RAX
            }),
            [0x48, 0x63, 0x04, 0x83]
        ); // movsxd rax,[rbx+rax*4]
        assert_eq!(
            enc(Insn::LoadI32Scaled {
                dst: RAX,
                base: R13,
                index: RAX
            }),
            [0x49, 0x63, 0x44, 0x85, 0x00]
        ); // movsxd rax,[r13+rax*4+0]: r13 can't be a disp-less base
        assert_eq!(
            enc(Insn::LoadI32Scaled {
                dst: Reg(8),
                base: R12,
                index: R9
            }),
            [0x4f, 0x63, 0x04, 0x8c]
        ); // movsxd r8,[r12+r9*4]: r9 as index needs REX.X
        assert_eq!(
            enc(Insn::LoadScaled8 {
                dst: RAX,
                base: RCX,
                index: RAX
            }),
            [0x48, 0x8b, 0x04, 0xc1]
        ); // mov rax,[rcx+rax*8]
        assert_eq!(
            enc(Insn::LoadScaled8 {
                dst: RSI,
                base: R13,
                index: RAX
            }),
            [0x49, 0x8b, 0x74, 0xc5, 0x00]
        ); // mov rsi,[r13+rax*8+0]
        assert_eq!(enc(Insn::JmpReg { reg: RAX }), [0xff, 0xe0]); // jmp rax
        assert_eq!(enc(Insn::JmpReg { reg: R11 }), [0x41, 0xff, 0xe3]); // jmp r11

        let mut code = Vec::new();
        jump(&mut code, Cond::AboveEq);
        jump(&mut code, Cond::Below);
        assert_eq!(code, [0x0f, 0x83, 0, 0, 0, 0, 0x0f, 0x82, 0, 0, 0, 0]); // jae, jb
    }

    #[test]
    /// Check the hand-written encoders for representative register and memory forms.
    fn reg_and_mem_encodings() {
        assert_eq!(
            enc(Insn::LoadMem {
                dst: Reg(0),
                base: Reg(1),
                offset: 8
            }),
            [0x48, 0x8b, 0x41, 0x08]
        ); // mov rax,[rcx+8]
        assert_eq!(
            enc(Insn::StoreMem {
                base: Reg(1),
                offset: 8,
                src: Reg(2)
            }),
            [0x48, 0x89, 0x51, 0x08]
        ); // mov [rcx+8],rdx
        assert_eq!(
            enc(Insn::LeaMem {
                dst: Reg(0),
                base: Reg(1),
                offset: 8
            }),
            [0x48, 0x8d, 0x41, 0x08]
        ); // lea rax,[rcx+8]
        assert_eq!(
            enc(Insn::LoadMem {
                dst: Reg(8),
                base: Reg(12),
                offset: 0
            }),
            [0x4d, 0x8b, 0x44, 0x24, 0x00]
        ); // mov r8,[r12+0]
        assert_eq!(
            enc(Insn::LoadMem {
                dst: Reg(0),
                base: Reg(1),
                offset: 128
            }),
            [0x48, 0x8b, 0x81, 0x80, 0x00, 0x00, 0x00]
        ); // mov rax,[rcx+128]
        assert_eq!(
            enc(Insn::Mov {
                dst: Reg(0),
                src: Reg(1)
            }),
            [0x48, 0x89, 0xc8]
        ); // mov rax,rcx
        assert_eq!(
            enc(Insn::Add {
                dst: Reg(0),
                src: Reg(2)
            }),
            [0x48, 0x01, 0xd0]
        ); // add rax,rdx
        assert_eq!(
            enc(Insn::Sub {
                dst: Reg(0),
                src: Reg(2)
            }),
            [0x48, 0x29, 0xd0]
        ); // sub rax,rdx
        assert_eq!(
            enc(Insn::Cmp {
                lhs: Reg(0),
                rhs: Reg(2)
            }),
            [0x48, 0x39, 0xd0]
        ); // cmp rax,rdx
        assert_eq!(
            enc(Insn::Imul {
                dst: Reg(0),
                src: Reg(2)
            }),
            [0x48, 0x0f, 0xaf, 0xc2]
        ); // imul rax,rdx
        assert_eq!(
            enc(Insn::SubImm {
                dst: Reg(0),
                imm: 1
            }),
            [0x48, 0x81, 0xe8, 1, 0, 0, 0]
        ); // sub rax,1
        assert_eq!(
            enc(Insn::Test {
                lhs: Reg(0),
                rhs: Reg(0)
            }),
            [0x48, 0x85, 0xc0]
        ); // test rax,rax
        assert_eq!(enc(Insn::Ret), [0xc3]);
        assert_eq!(
            enc(Insn::CallRel { disp: -0x40 }),
            [0xe8, 0xc0, 0xff, 0xff, 0xff]
        );
    }
}
