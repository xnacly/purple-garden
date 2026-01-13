use crate::vm::BuiltinFn;

#[derive(Debug)]
#[allow(unpredictable_function_pointer_comparisons)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub enum Op<'vm> {
    Add {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    Sub {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    Mul {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    Div {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    Eq {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    Lt {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    Gt {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    Not {
        dst: u8,
        src: u8,
    },
    Mov {
        dst: u8,
        src: u8,
    },
    LoadImm {
        dst: u8,
        value: i64,
    },
    LoadGlobal {
        dst: u8,
        idx: u32,
    },
    Jmp {
        target: u16,
    },
    JmpF {
        cond: u8,
        target: u16,
    },
    Call {
        func: u16,
        args_start: u8,
        args_len: u8,
    },
    Sys {
        ptr: BuiltinFn<'vm>,
        args_start: u8,
        args_len: u8,
    },
    Ret,
}
