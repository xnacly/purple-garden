use crate::{
    asm::{aarch64, x86},
    vm,
};

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
            offset: size_of::<vm::Value>() as i32,
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
            offset: size_of::<vm::Value>() as i32,
        },
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rdx,
            base: x86::Reg::Rbx,
            offset: 2 * size_of::<vm::Value>() as i32,
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
            offset: size_of::<vm::Value>() as i32,
        },
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rdx,
            base: x86::Reg::Rbx,
            offset: 2 * size_of::<vm::Value>() as i32,
        },
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rcx,
            base: x86::Reg::Rbx,
            offset: 3 * size_of::<vm::Value>() as i32,
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
            offset: size_of::<vm::Value>() as i32,
        },
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rdx,
            base: x86::Reg::Rbx,
            offset: 2 * size_of::<vm::Value>() as i32,
        },
        x86::Instruction::MovRegMem {
            dst: x86::Reg::Rcx,
            base: x86::Reg::Rbx,
            offset: 3 * size_of::<vm::Value>() as i32,
        },
        x86::Instruction::MovRegMem {
            dst: x86::Reg::R8,
            base: x86::Reg::Rbx,
            offset: 4 * size_of::<vm::Value>() as i32,
        },
    ],
];

// TODO: FUNCTION_PRELUDE_AARCH64
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
