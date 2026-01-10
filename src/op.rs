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
    LoadLocal {
        slot: u16,
        dst: u8,
    },
    StoreLocal {
        slot: u16,
        src: u8,
    },
    Size {
        dst: u8,
        value: u32,
    },
    New {
        dst: u8,
        size: u8,
        new_type: New,
    },
    Append {
        container: u8,
        src: u8,
    },
    Len {
        dst: u8,
        src: u8,
    },
    Idx {
        dst: u8,
        container: u8,
        index: u8,
    },
    Jmp {
        target: usize,
    },
    JmpF {
        cond: u8,
        target: usize,
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
    /// specifically for tailcall optimisation, see https://en.wikipedia.org/wiki/Tail_call
    Tail {
        func: u16,
        args_start: u8,
        args_len: u8,
    },
    RetMultiple {
        /// used for peephole optimisation, merging multiple RET into a single RET with a count
        times: u8,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum New {
    Object,
    Array,
}
