#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    IAdd { dst: u8, lhs: u8, rhs: u8 },
    ISub { dst: u8, lhs: u8, rhs: u8 },
    IMul { dst: u8, lhs: u8, rhs: u8 },
    IDiv { dst: u8, lhs: u8, rhs: u8 },
    ILt { dst: u8, lhs: u8, rhs: u8 },
    IGt { dst: u8, lhs: u8, rhs: u8 },
    IEq { dst: u8, lhs: u8, rhs: u8 },
    DAdd { dst: u8, lhs: u8, rhs: u8 },
    DSub { dst: u8, lhs: u8, rhs: u8 },
    DMul { dst: u8, lhs: u8, rhs: u8 },
    DDiv { dst: u8, lhs: u8, rhs: u8 },
    DLt { dst: u8, lhs: u8, rhs: u8 },
    DGt { dst: u8, lhs: u8, rhs: u8 },
    BEq { dst: u8, lhs: u8, rhs: u8 },

    Mov { dst: u8, src: u8 },

    LoadI { dst: u8, value: i32 },
    LoadG { dst: u8, idx: u32 },

    Jmp { target: u16 },
    JmpF { cond: u8, target: u16 },

    Call { func: u32 },
    Sys { idx: u8 },
    Push { src: u8 },
    Pop { dst: u8 },

    CastToInt { dst: u8, src: u8 },
    CastToDouble { dst: u8, src: u8 },
    CastToBool { dst: u8, src: u8 },

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
