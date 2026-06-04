//! x86-64 JIT lowering.
//!
//! A peer backend to the bytecode compiler: lowers IR straight to native
//! code, with SSA values living in real x86 GPRs (its own [`crate::regalloc`]
//! over the shared IR liveness). The native ABI passes `*mut Vm` in `rdi`, and
//! since `Vm::r` is the first field, `rdi` is the base of the VM register file.
//! Args arrive in `vm.r[0..n]`; the result is written back to `vm.r[0]`. Because
//! all computation happens in GPRs, `vm.r[1..]` is never touched, so the
//! syscall convention ("a syscall changes only r0") holds for free.

use std::fmt;

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
/// Pool for `idiv` functions; rax/rcx/rdx reserved as its fixed scratch, so
/// fewer regs and likelier to spill back to bytecode.
const POOL_DIV: &[u8] = &[6, 8, 9, 10, 11]; // rsi r8 r9 r10 r11
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

/// A single x86-64 instruction. `encode` appends its machine-code bytes;
/// `Display` renders it as readable assembly (the JIT's own disassembler).
/// Register fields are physical GPR numbers (0=rax .. 15=r15); `slot` indexes
/// the VM register file at `[rdi + slot*8]`.
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
    pub fn encode(self, code: &mut Vec<u8>) {
        match self {
            Insn::Ret => code.push(0xc3),
            Insn::LoadSlot { dst, slot } => mov_slot(code, 0x8b, dst, slot),
            Insn::StoreSlot { src, slot } => mov_slot(code, 0x89, src, slot),
            // 0x89 = `mov r/m, r`: r/m is dst, reg is src.
            Insn::Mov { dst, src } => reg_reg(code, 0x89, src, dst),
            // 0x01 = `add r/m, r`, 0x29 = `sub r/m, r`: r/m is dst, reg is src.
            Insn::Add { dst, src } => reg_reg(code, 0x01, src, dst),
            Insn::Sub { dst, src } => reg_reg(code, 0x29, src, dst),
            // 0x0f 0xaf = `imul r, r/m`: reg is dst, r/m is src (operand order
            // is the opposite of add/sub).
            Insn::Imul { dst, src } => {
                code.push(rex(dst, src));
                code.extend_from_slice(&[0x0f, 0xaf, modrm(dst, src)]);
            }
            // 0xf7 /3 ; neg r/m64.
            Insn::Neg { reg } => {
                code.push(rex(0, reg));
                code.extend_from_slice(&[0xf7, modrm(3, reg)]);
            }
            // 0x81 /0 add, /5 sub, /7 cmp; r/m, imm32.
            Insn::AddImm { dst, imm } => reg_imm(code, 0, dst, imm),
            Insn::SubImm { dst, imm } => reg_imm(code, 5, dst, imm),
            Insn::AndImm { dst, imm } => reg_imm(code, 4, dst, imm),
            Insn::CmpImm { reg, imm } => reg_imm(code, 7, reg, imm),
            Insn::Test { lhs, rhs } => reg_reg(code, 0x85, rhs, lhs),
            // 0xc7 /0 ; mov r/m, imm32.
            Insn::MovImm { dst, imm } => {
                code.push(rex(0, dst));
                code.push(0xc7);
                code.push(modrm(0, dst));
                code.extend_from_slice(&imm.to_le_bytes());
            }
            // 0x0f 0x94 ; setcc(e) r/m8. The bare-or-extended REX lets us name
            // sil/r8b etc. as byte registers.
            Insn::Sete { dst } => {
                code.push(0x40 | u8::from(dst >= 8));
                code.extend_from_slice(&[0x0f, 0x94, modrm(0, dst)]);
            }
            // REX.W 0xb8+rd io64 ; movabs r64, imm64.
            Insn::MovAbs { dst, imm } => {
                code.push(0x48 | u8::from(dst >= 8));
                code.push(0xb8 + (dst & 7));
                code.extend_from_slice(&imm.to_le_bytes());
            }
            // 0xff /2 ; call r/m64. REX.B reaches r8..r15.
            Insn::CallReg { reg } => {
                if reg >= 8 {
                    code.push(0x41);
                }
                code.extend_from_slice(&[0xff, modrm(2, reg)]);
            }
            // REX.W 0x99 ; cqo.
            Insn::Cqo => code.extend_from_slice(&[0x48, 0x99]),
            // REX.W 0xf7 /7 ; idiv r/m64.
            Insn::Idiv { divisor } => {
                code.push(rex(0, divisor));
                code.extend_from_slice(&[0xf7, modrm(7, divisor)]);
            }
        }
    }
}

#[inline]
fn emit(code: &mut Vec<u8>, insn: Insn) {
    insn.encode(code);
}

/// 64-bit GPR name for a physical register number.
fn reg_name(r: u8) -> &'static str {
    [
        "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12",
        "r13", "r14", "r15",
    ][r as usize]
}

impl fmt::Display for Insn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let r = reg_name;
        match *self {
            Insn::Ret => write!(f, "ret"),
            Insn::LoadSlot { dst, slot } => write!(f, "mov {}, [rdi+{:#x}]", r(dst), slot * 8),
            Insn::StoreSlot { src, slot } => write!(f, "mov [rdi+{:#x}], {}", slot * 8, r(src)),
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
            Insn::Test { lhs, rhs } => write!(f, "test {}, {}", r(lhs), r(rhs)),
            Insn::Sete { dst } => write!(f, "sete {}b", r(dst)),
            Insn::MovAbs { dst, imm } => write!(f, "movabs {}, {imm:#x}", r(dst)),
            Insn::CallReg { reg } => write!(f, "call {}", r(reg)),
            Insn::Cqo => write!(f, "cqo"),
            Insn::Idiv { divisor } => write!(f, "idiv {}", r(divisor)),
        }
    }
}

/// REX.W prefix, plus REX.R if the `reg` field is r8..r15 and REX.B if the `rm`
/// field is r8..r15.
fn rex(reg: u8, rm: u8) -> u8 {
    0x48 | (u8::from(reg >= 8) << 2) | u8::from(rm >= 8)
}

/// ModRM byte for the register-direct form (mod=11): `reg` in bits 3..6,
/// `rm` in bits 0..2.
fn modrm(reg: u8, rm: u8) -> u8 {
    0xc0 | ((reg & 7) << 3) | (rm & 7)
}

/// Register-register op: `REX.W opcode ModRM(reg, rm)`.
fn reg_reg(code: &mut Vec<u8>, opcode: u8, reg: u8, rm: u8) {
    code.extend_from_slice(&[rex(reg, rm), opcode, modrm(reg, rm)]);
}

/// Register-immediate op: `REX.W 0x81 /digit imm32`.
fn reg_imm(code: &mut Vec<u8>, digit: u8, rm: u8, imm: i32) {
    code.push(rex(0, rm));
    code.push(0x81);
    code.push(modrm(digit, rm));
    code.extend_from_slice(&imm.to_le_bytes());
}

/// `mov` between GPR `reg` and `[rdi + slot*8]` (opcode 0x8b load, 0x89 store).
fn mov_slot(code: &mut Vec<u8>, opcode: u8, reg: u8, slot: u8) {
    // ModRM mod=01 (disp8), reg field = GPR, rm = rdi.
    let m = 0x40 | ((reg & 7) << 3) | RDI;
    code.extend_from_slice(&[rex(reg, RDI), opcode, m, slot * 8]);
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
/// form, so the divisor goes via rcx. Caller allocates l/d from `POOL_DIV`.
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

pub fn compile_func(
    func: &ir::Func<'_>,
    out: &mut Vec<u8>,
    liveness: &[(u32, u32)],
    allocator: &mut crate::regalloc::Allocator,
) -> Option<()> {
    let mut blocks = func.blocks.iter().filter(|block| !block.tombstone);
    let Some(block) = blocks.next() else {
        skip!(func, "empty function");
    };
    if blocks.next().is_some() {
        skip!(func, "multiple blocks");
    }

    // Constant-divisor IDiv/IMod (not imm 0, which traps, nor mod 2, which uses
    // `and`) lowers to `idiv` and needs rax/rcx/rdx reserved.
    let needs_idiv = block.instructions.iter().any(|i| {
        matches!(i, ir::Instr::BinImm { op: BinOp::IDiv, imm, .. } if *imm != 0)
            || matches!(i, ir::Instr::BinImm { op: BinOp::IMod, imm, .. } if *imm != 0 && *imm != 2)
    });
    let caller = if needs_idiv { POOL_DIV } else { POOL };
    // No calls lowered yet, so call_sites is empty and the callee class is unused.
    let regs = allocator.rebuild(
        liveness,
        &[],
        crate::regalloc::RegClasses {
            caller,
            callee: POOL_CALLEE,
        },
    );
    let reg = |id: ir::Id| match regs.get(id.0 as usize) {
        Some(ir::Location::Reg(r)) => Some(*r),
        _ => None,
    };

    // Args arrive in the VM register file: param i in vm.r[i] == [rdi + i*8].
    for (i, &param) in func.params.iter().enumerate() {
        if let Some(r) = reg(param) {
            emit(
                out,
                Insn::LoadSlot {
                    dst: r,
                    slot: i as u8,
                },
            );
        }
    }

    for instr in &block.instructions {
        match instr {
            ir::Instr::Noop => {}
            ir::Instr::LoadConst { dst, value, .. } => {
                let Some(dst) = reg(dst.id) else {
                    unreachable!();
                };
                let imm = match value {
                    purple_garden_ir::Const::False => 0,
                    purple_garden_ir::Const::True => 1,
                    purple_garden_ir::Const::Int(i)
                        if (*i as i32) < i32::MAX && (*i as i32) > i32::MIN =>
                    {
                        *i as i32
                    }
                    _ => skip!(func, "const not true, false or i32::MIN < i < i32::MAX"),
                };

                emit(out, Insn::MovImm { dst, imm })
            }
            ir::Instr::BinImm {
                op, dst, lhs, imm, ..
            } => {
                let (Some(d), Some(l)) = (reg(dst.id), reg(*lhs)) else {
                    unreachable!();
                };
                let imm = *imm;
                match op {
                    // dst = lhs <op> imm. Get lhs into dst, then op in place.
                    BinOp::IAdd | BinOp::ISub => {
                        if d != l {
                            emit(out, Insn::Mov { dst: d, src: l });
                        }
                        emit(
                            out,
                            match op {
                                BinOp::IAdd => Insn::AddImm { dst: d, imm },
                                _ => Insn::SubImm { dst: d, imm },
                            },
                        );
                    }
                    // dst = (lhs == imm) as 0/1. cmp reads lhs and sets flags;
                    // mov (no flags) clears dst; sete writes the low byte. Safe
                    // even if dst == lhs (the cmp happens before the mov).
                    BinOp::IEq => {
                        if imm == 0 {
                            emit(out, Insn::Test { lhs: l, rhs: l });
                        } else {
                            emit(out, Insn::CmpImm { reg: l, imm });
                        }
                        emit(out, Insn::MovImm { dst: d, imm: 0 });
                        emit(out, Insn::Sete { dst: d });
                    }
                    // static divide-by-zero; trap and return, the rest is dead.
                    BinOp::IDiv | BinOp::IMod if imm == 0 => {
                        let helper: purple_garden_runtime::BuiltinFn =
                            purple_garden_runtime::jit_trap_div_zero;
                        emit_abi_call(out, helper as usize as u64);
                        emit(out, Insn::Ret);
                        return Some(());
                    }
                    // x % 2, non-negative dividend; mask the low bit.
                    BinOp::IMod if imm == 2 => {
                        if d != l {
                            emit(out, Insn::Mov { dst: d, src: l });
                        }
                        emit(out, Insn::AndImm { dst: d, imm: 1 });
                    }
                    BinOp::IDiv | BinOp::IMod => emit_idiv(out, *op, d, l, imm),
                    _ => skip!(func, "unsupported binimm op {op:?}"),
                }
            }
            ir::Instr::Bin {
                op, dst, lhs, rhs, ..
            } => {
                let (Some(d), Some(l), Some(r)) = (reg(dst.id), reg(*lhs), reg(*rhs)) else {
                    skip!(func, "unallocated operand in {instr:?}");
                };
                let op = match op {
                    BinOp::IAdd | BinOp::ISub | BinOp::IMul => *op,
                    _ => skip!(func, "unsupported bin op {op:?}"),
                };
                emit_bin(out, op, d, l, r);
            }
            _ => skip!(func, "unsupported instruction {instr:?}"),
        }
    }

    let term = block.term.as_ref();
    match term {
        None => {}
        Some(ir::Terminator::Return { value, .. }) => {
            if let Some(value) = value {
                let Some(r) = reg(*value) else {
                    skip!(func, "return value %v{} unallocated", value.0);
                };
                emit(out, Insn::StoreSlot { src: r, slot: 0 }); // result -> vm.r[0]
            }
        }
        _ => skip!(func, "unsupported terminator {term:?}"),
    }
    emit(out, Insn::Ret);

    purple_garden_shared::trace!("[jit::x86] compiled {} ({} bytes)", func.name, out.len());
    Some(())
}

#[cfg(test)]
mod tests {
    use super::Insn;

    fn enc(insn: Insn) -> Vec<u8> {
        let mut code = Vec::new();
        insn.encode(&mut code);
        code
    }

    #[test]
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
        assert_eq!(enc(Insn::Mov { dst: 0, src: 1 }), [0x48, 0x89, 0xc8]); // mov rax,rcx
        assert_eq!(enc(Insn::Add { dst: 0, src: 2 }), [0x48, 0x01, 0xd0]); // add rax,rdx
        assert_eq!(enc(Insn::Sub { dst: 0, src: 2 }), [0x48, 0x29, 0xd0]); // sub rax,rdx
        assert_eq!(enc(Insn::Imul { dst: 0, src: 2 }), [0x48, 0x0f, 0xaf, 0xc2]); // imul rax,rdx
        assert_eq!(
            enc(Insn::SubImm { dst: 0, imm: 1 }),
            [0x48, 0x81, 0xe8, 1, 0, 0, 0]
        ); // sub rax,1
        assert_eq!(enc(Insn::Test { lhs: 0, rhs: 0 }), [0x48, 0x85, 0xc0]); // test rax,rax
        assert_eq!(enc(Insn::Ret), [0xc3]);
    }
}
