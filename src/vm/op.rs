#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    IAdd {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    ISub {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    IMul {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    IDiv {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    ILt {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    IGt {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    IEq {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    DAdd {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    DSub {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    DMul {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    DDiv {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    DLt {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    DGt {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    BEq {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },

    Mov {
        dst: u8,
        src: u8,
    },

    LoadI {
        dst: u8,
        value: i32,
    },
    LoadG {
        dst: u8,
        idx: u32,
    },

    Jmp {
        target: u16,
    },
    /// Conditional jump: branch to `target` when `cond` is *truthy*.
    /// Falls through when `cond` is zero.
    JmpT {
        cond: u8,
        target: u16,
    },
    /// Tail call: jump to `func` (an absolute pc) without growing the
    /// callstack. Same calling convention as [Op::Call].
    Tail {
        func: u32,
    },

    /// Call the function whose bytecode starts at pc `func`.
    ///
    /// Calling convention (codegen relies on this):
    /// - Args are passed in `r0..r{argcount-1}`, set up by the parallel-move
    ///   resolver in `bc::Cc::emit_arg_shuffle` before the [Op::Call] op.
    /// - The result is returned in `r0`; the caller copies it to its real
    ///   destination via a subsequent `Mov`.
    /// - All registers are caller-save. The callee may freely overwrite
    ///   any register, so the bc emitter spills every value that's alive
    ///   across this call (see the `alive_after_call_spill` loop in
    ///   `bc::Cc::instr`, `Instr::Call`) and restores them after the call
    ///   via [Op::Pop].
    /// - The dispatcher pushes a [CallFrame] containing the return pc; the
    ///   matching [Op::Ret] pops it and resumes.
    Call {
        func: u32,
    },
    /// Invoke syscall. `idx` (is the index into [Vm::syscalls]). See
    /// [crate::vm::BuiltinFn] for the syscall calling convention. It's
    /// stricter than [Op::Call] (only r0 is clobbered by the body itself).
    Sys {
        idx: u16,
    },
    /// Push `src` onto [Vm::spilled]. Used both for caller-save spill
    /// around [Op::Call] / [Op::Sys] and for cycle-breaking inside
    /// `bc::Cc::emit_arg_shuffle`.
    ///
    /// Invariant: every function's bytecode must leave [Vm::spilled] at
    /// the same depth on [Op::Ret] as on entry. Unbalanced push/pop pairs
    /// silently corrupt the caller's spilled values; the debug check on
    /// [Op::Ret] catches this in dev builds.
    Push {
        src: u8,
    },
    /// Pop the top of [Vm::spilled] into `dst`. See [Op::Push] for the
    /// stack-balance invariant.
    Pop {
        dst: u8,
    },

    CastToInt {
        dst: u8,
        src: u8,
    },
    CastToDouble {
        dst: u8,
        src: u8,
    },
    /// Reads `src` as a u64 and stores `!= 0`. Not valid for f64 inputs:
    /// -0.0 and NaN would land as `true`.
    CastToBool {
        dst: u8,
        src: u8,
    },

    Ret,
    Nop,
}

#[cfg(test)]
mod op_test {
    #[test]
    fn op_size_8_byte() {
        assert_eq!(std::mem::size_of::<crate::vm::op::Op>(), 8);
    }
}
