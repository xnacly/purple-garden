#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    // TODO: rename all to I* for integers, introduce secondary operator for D* (double)
    IAdd { dst: u8, lhs: u8, rhs: u8 },
    ISub { dst: u8, lhs: u8, rhs: u8 },
    IMul { dst: u8, lhs: u8, rhs: u8 },
    IDiv { dst: u8, lhs: u8, rhs: u8 },
    Eq { dst: u8, lhs: u8, rhs: u8 },
    Lt { dst: u8, lhs: u8, rhs: u8 },
    Gt { dst: u8, lhs: u8, rhs: u8 },
    Mov { dst: u8, src: u8 },

    LoadI { dst: u8, value: i32 },
    LoadG { dst: u8, idx: u32 },
    Jmp { target: u16 },
    JmpF { cond: u8, target: u16 },
    Call { func: u32 },
    Sys { idx: u8 },
    Push { src: u8 },
    Pop { dst: u8 },
    Ret,
    Nop,
}

#[cfg(test)]
mod op {

    #[test]
    fn op_size_8_byte() {
        assert_eq!(std::mem::size_of::<crate::vm::op::Op>(), 8);
    }
}
