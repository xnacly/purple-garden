#[derive(Debug)]
#[allow(unpredictable_function_pointer_comparisons)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub enum Op {
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
        value: i32,
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
        idx: u8,
        args_start: u8,
        args_len: u8,
    },
    Ret,
    Nop,
}

#[cfg(test)]
mod op {

    #[test]
    fn op_size_8_byte() {
        assert_eq!(std::mem::size_of::<crate::op::Op>(), 8);
    }
}
