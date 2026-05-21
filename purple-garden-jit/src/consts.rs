use crate::asm::{aarch64, x86};

const VALUE_SIZE: i32 = 8;

#[allow(unused)]
/// Preludes for 0-5 amount of arguments passed to any given JIT function
/// Rbx points to the start of `Vm::r`
pub const FUNCTION_PRELUDES_X86: [&[x86::Instruction]; 6] = [
    // 0 args
    &[],
    // 1 arg
    &[x86::Instruction::MovRegMem {
        dst: x86::Reg::Rdi,
        base: x86::Reg::Rbx,
        offset: 0,
    }],
    // 2 args
    &[
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rdi,
            base: x86::Reg::Rbx,
            offset: 0,
        },
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rsi,
            base: x86::Reg::Rbx,
            offset: VALUE_SIZE,
        },
    ],
    // 3 args
    &[
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rdi,
            base: x86::Reg::Rbx,
            offset: 0,
        },
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rsi,
            base: x86::Reg::Rbx,
            offset: VALUE_SIZE,
        },
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rdx,
            base: x86::Reg::Rbx,
            offset: 2 * VALUE_SIZE,
        },
    ],
    // 4 args
    &[
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rdi,
            base: x86::Reg::Rbx,
            offset: 0,
        },
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rsi,
            base: x86::Reg::Rbx,
            offset: VALUE_SIZE,
        },
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rdx,
            base: x86::Reg::Rbx,
            offset: 2 * VALUE_SIZE,
        },
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rcx,
            base: x86::Reg::Rbx,
            offset: 3 * VALUE_SIZE,
        },
    ],
    // 5 args
    &[
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rdi,
            base: x86::Reg::Rbx,
            offset: 0,
        },
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rsi,
            base: x86::Reg::Rbx,
            offset: VALUE_SIZE,
        },
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rdx,
            base: x86::Reg::Rbx,
            offset: 2 * VALUE_SIZE,
        },
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rcx,
            base: x86::Reg::Rbx,
            offset: 3 * VALUE_SIZE,
        },
        x86::Instruction::MovRegMem {
            dst: x86::Reg::R8,
            base: x86::Reg::Rbx,
            offset: 4 * VALUE_SIZE,
        },
    ],
];

#[allow(unused)]
pub const FUNCTION_PRELUDE_AARCH64: [&[aarch64::Instruction]; 6] = [
    // 0 args
    &[],
    // 1 arg
    &[],
    // 2 args
    &[],
    // 3 args
    &[],
    // 4 args
    &[],
    // 5 args
    &[],
];
