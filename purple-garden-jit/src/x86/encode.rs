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

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reg(pub u8);

pub const RAX: Reg = Reg(0);
pub const RCX: Reg = Reg(1);
pub const RDX: Reg = Reg(2);
pub const RBX: Reg = Reg(3);
pub const RSP: Reg = Reg(4);
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
pub fn jump(code: &mut Vec<u8>, cond: Cond) -> usize {
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
///
/// `slot` indexes the VM register file at `[rdi + slot*8]`.
#[derive(Debug, Clone, Copy)]
pub enum Insn {
    Ret,
    /// `mov r{dst}, [rdi + slot*8]`
    LoadSlot {
        dst: Reg,
        slot: u8,
    },
    /// `mov [rdi + slot*8], r{src}`
    StoreSlot {
        src: Reg,
        slot: u8,
    },
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
    /// `test r{lhs}, r{rhs}`
    Test {
        lhs: Reg,
        rhs: Reg,
    },
    /// `sete r{dst}b`; set r{dst}'s low byte to 1 if the last compare was equal.
    Sete {
        dst: Reg,
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
}

impl Insn {
    /// Append this instruction's x86-64 machine-code bytes to `code`.
    pub fn encode(self, code: &mut Vec<u8>) {
        match self {
            Insn::Ret => code.push(0xc3),
            Insn::LoadSlot { dst, slot } => mov_slot(code, 0x8b, dst.0, slot),
            Insn::StoreSlot { src, slot } => mov_slot(code, 0x89, src.0, slot),
            // Record memory ops all use the same ModRM/SIB memory form. The
            // opcode selects load, store, or address calculation; ModRM.reg is
            // the register operand for all three encodings.
            Insn::LoadMem { dst, base, offset } => mem_disp(code, 0x8b, dst.0, base.0, offset),
            Insn::StoreMem { base, offset, src } => mem_disp(code, 0x89, src.0, base.0, offset),
            Insn::LeaMem { dst, base, offset } => mem_disp(code, 0x8d, dst.0, base.0, offset),
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
            // 0x0f 0x94 = `sete r/m8`.
            // The REX prefix is not REX.W here; it exists only so byte-register
            // names are the modern low-byte registers (`sil`, `dil`, `r8b`, ...).
            // ModRM.reg is /0, ModRM.r/m names the byte destination.
            Insn::Sete { dst } => {
                code.push(0x40 | u8::from(dst.0 >= 8));
                code.extend_from_slice(&[0x0f, 0x94, modrm(0, dst.0)]);
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
        }
    }
}

impl fmt::Display for Insn {
    /// Render one instruction as the JIT's readable assembly format.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Insn::Ret => write!(f, "ret"),
            Insn::LoadSlot { dst, slot } => write!(f, "mov {}, [rdi+{:#x}]", dst, slot * 8),
            Insn::StoreSlot { src, slot } => write!(f, "mov [rdi+{:#x}], {}", slot * 8, src),
            Insn::LoadMem { dst, base, offset } => {
                write!(f, "mov {}, [{}+{:#x}]", dst, base, offset)
            }
            Insn::StoreMem { base, offset, src } => {
                write!(f, "mov [{}+{:#x}], {}", base, offset, src)
            }
            Insn::LeaMem { dst, base, offset } => {
                write!(f, "lea {}, [{}+{:#x}]", dst, base, offset)
            }
            Insn::Mov { dst, src } => write!(f, "mov {}, {}", dst, src),
            Insn::MovImm { dst, imm } => write!(f, "mov {}, {imm}", dst),
            Insn::Add { dst, src } => write!(f, "add {}, {}", dst, src),
            Insn::Sub { dst, src } => write!(f, "sub {}, {}", dst, src),
            Insn::Imul { dst, src } => write!(f, "imul {}, {}", dst, src),
            Insn::Neg { reg } => write!(f, "neg {}", reg),
            Insn::AddImm { dst, imm } => write!(f, "add {}, {imm}", dst),
            Insn::SubImm { dst, imm } => write!(f, "sub {}, {imm}", dst),
            Insn::AndImm { dst, imm } => write!(f, "and {}, {imm}", dst),
            Insn::CmpImm { reg, imm } => write!(f, "cmp {}, {imm}", reg),
            Insn::Cmp { lhs, rhs } => write!(f, "cmp {}, {}", lhs, rhs),
            Insn::Test { lhs, rhs } => write!(f, "test {}, {}", lhs, rhs),
            Insn::Sete { dst } => write!(f, "sete {}b", dst),
            Insn::MovAbs { dst, imm } => write!(f, "movabs {}, {imm:#x}", dst),
            Insn::CallReg { reg } => write!(f, "call {}", reg),
            Insn::Push { reg } => write!(f, "push {}", reg),
            Insn::Pop { reg } => write!(f, "pop {}", reg),
            Insn::ShrImm { dst, imm } => write!(f, "shr {}, {imm}", dst),
            Insn::LeaRip { dst, disp } => write!(f, "lea {}, [rip+{disp:#x}]", dst),
            Insn::LoadI32Scaled { dst, base, index } => {
                write!(f, "movsxd {}, dword [{}+{}*4]", dst, base, index)
            }
            Insn::LoadScaled8 { dst, base, index } => {
                write!(f, "mov {}, qword [{}+{}*8]", dst, base, index)
            }
            Insn::JmpReg { reg } => write!(f, "jmp {}", reg),
            Insn::Cqo => write!(f, "cqo"),
            Insn::Idiv { divisor } => write!(f, "idiv {}", divisor),
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
/// This is why the lowering rejects more than 32 params: `slot*8`
/// must fit in a signed 8-bit displacement for this compact addressing form.
fn mov_slot(code: &mut Vec<u8>, opcode: u8, reg: u8, slot: u8) {
    // ModRM mod=01 (disp8), reg field = GPR, rm = rdi.
    let m = 0x40 | ((reg & 7) << 3) | RDI.0;
    code.extend_from_slice(&[rex(reg, RDI.0), opcode, m, slot * 8]);
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
    let rm = if needs_sib(base) { RSP.0 } else { base & 7 };
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

/// `opcode r64, [base + index*(1 << scale)]`: ModRM r/m=100 means a SIB byte
/// follows. The index register's high bit is REX.X, which [`rex`] doesn't set.
/// A base with low bits 101 (rbp/r13) and mod=00 would mean "no base, disp32",
/// so those take mod=01 with a zero disp8.
fn scaled(code: &mut Vec<u8>, opcode: u8, scale: u8, dst: Reg, base: Reg, index: Reg) {
    let rex = rex(dst.0, base.0) | (u8::from(index.0 >= 8) << 1);
    let mode = if base.0 & 7 == 0b101 { 0x40 } else { 0x00 };
    let sib = (scale << 6) | ((index.0 & 7) << 3) | (base.0 & 7);
    code.extend_from_slice(&[rex, opcode, mode | ((dst.0 & 7) << 3) | 0b100, sib]);
    if mode == 0x40 {
        code.push(0);
    }
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
    /// Check the hand-written encoders for representative register and slot forms.
    fn slot_and_reg_encodings() {
        assert_eq!(
            enc(Insn::LoadSlot {
                dst: Reg(0),
                slot: 0
            }),
            [0x48, 0x8b, 0x47, 0x00]
        ); // mov rax,[rdi+0]
        assert_eq!(
            enc(Insn::StoreSlot {
                src: Reg(1),
                slot: 1
            }),
            [0x48, 0x89, 0x4f, 0x08]
        ); // mov [rdi+8],rcx
        assert_eq!(
            enc(Insn::LoadSlot {
                dst: Reg(8),
                slot: 2
            }),
            [0x4c, 0x8b, 0x47, 0x10]
        ); // mov r8,[rdi+16]
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
    }
}
