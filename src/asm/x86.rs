use crate::jit;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum Reg {
    Rax,
    Rbx,
    Rcx,
    Rdx,
    Rsi,
    Rdi,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
}

#[derive(Debug)]
pub enum Instruction {
    MovRegReg {
        dst: Reg,
        src: Reg,
    },
    MovRegImm {
        dst: Reg,
        imm: i64,
    },
    AddRegReg {
        dst: Reg,
        src: Reg,
    },
    SubRegReg {
        dst: Reg,
        src: Reg,
    },
    MulRegReg {
        dst: Reg,
        src: Reg,
    }, // signed multiply
    DivRegReg {
        dst: Reg,
        src: Reg,
    }, // signed divide (rax / src → dst)
    CmpRegReg {
        lhs: Reg,
        rhs: Reg,
    },
    SetEq {
        dst: Reg,
    },
    SetLt {
        dst: Reg,
    },
    SetGt {
        dst: Reg,
    },
    NotReg {
        dst: Reg,
    },
    PushReg {
        src: Reg,
    },
    PopReg {
        dst: Reg,
    },
    Call {
        addr: u64,
    },
    Ret,
    Nop,
    /// `dst = [base+offset]`
    MovRegMem {
        dst: Reg,
        base: Reg,
        offset: i32,
    },
    /// `[dst_base+offset] = src`
    MovMemReg {
        dst_base: Reg,
        offset: i32,
        src: Reg,
    },
}

impl Instruction {
    pub fn encode(self, jit: &mut jit::Bjit) {
        todo!()
    }
}
