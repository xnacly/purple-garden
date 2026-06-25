use crate::AllocType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    IAdd {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    /// `r[dst] = r[lhs] + imm`. Lowered from IR `BinImm`
    /// (commutative; either side of the original `IAdd` can be the constant).
    IAddI {
        dst: u8,
        lhs: u8,
        imm: i32,
    },
    ISub {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    /// `r[dst] = r[lhs] - imm`. Lowered from IR `BinImm`, but only when
    /// the constant sat on the `ISub`'s rhs side
    /// (subtraction isn't commutative and we don't currently emit an
    /// `imm - r[x]` form).
    ISubI {
        dst: u8,
        lhs: u8,
        imm: i32,
    },
    IMul {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    /// `r[dst] = r[lhs] * imm`. Lowered from IR `BinImm`
    /// (commutative).
    IMulI {
        dst: u8,
        lhs: u8,
        imm: i32,
    },
    IDiv {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    /// `r[dst] = r[lhs] / imm`. Lowered from IR `BinImm` when the
    /// constant was the divisor (rhs). Still traps on
    /// `imm == 0` to preserve `IDiv` semantics for code that statically
    /// divides by zero.
    IDivI {
        dst: u8,
        lhs: u8,
        imm: i32,
    },
    IMod {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    /// `r[dst] = r[lhs] % imm`. Lowered from IR `BinImm` when the
    /// constant was the divisor (rhs). Traps on `imm == 0`, like `IDivI`.
    IModI {
        dst: u8,
        lhs: u8,
        imm: i32,
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
    IEqI {
        dst: u8,
        lhs: u8,
        imm: i32,
    },
    /// `r[dst] = r[lhs] > imm`. Lowered from IR `BinImm`, including
    /// swapped constant-lhs comparisons.
    IGtI {
        dst: u8,
        lhs: u8,
        imm: i32,
    },
    /// `r[dst] = r[lhs] < imm`. Lowered from IR `BinImm`, including
    /// swapped constant-lhs comparisons.
    ILtI {
        dst: u8,
        lhs: u8,
        imm: i32,
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
    /// Conditional jump: branch to `target` when `cond` is *falsy*.
    /// Falls through when `cond` is truthy. Lets a two-armed branch
    /// fuse its `JmpT yes; Jmp no` pair into a single op when `yes` is
    /// the fall-through block.
    JmpF {
        cond: u8,
        target: u16,
    },
    /// Conditional jump: branch when `r[lhs] == imm`.
    JmpEqI {
        lhs: u8,
        imm: i32,
        target: u16,
    },
    /// Conditional jump: branch when `r[lhs] != imm`.
    JmpNeI {
        lhs: u8,
        imm: i32,
        target: u16,
    },
    /// Tail call: jump to `func` (an absolute pc) without growing the
    /// callstack. Same calling convention as [`Op::Call`].
    Tail {
        func: u32,
    },

    /// Call the function whose bytecode starts at pc `func`.
    ///
    /// Calling convention (codegen relies on this):
    /// - Args are passed in `r0..r{argcount-1}`, set up by the parallel-move
    ///   resolver in `bc::Cc::emit_arg_shuffle` before the [`Op::Call`] op.
    /// - `r0` is both the first argument and the return-value slot (ARM-like).
    /// - The result is returned in `r0`; the caller copies it to its real
    ///   destination via a subsequent `Mov`.
    /// - Callee-saved: the callee's prologue pushes `r1..r{max_reg}` and the
    ///   epilogue pops them before every [`Op::Ret`] / [`Op::Tail`]. The caller
    ///   only spills values in `r0..r{argcount-1}` (the arg-shuffle zone).
    /// - The dispatcher pushes a [`CallFrame`] containing the return pc; the
    ///   matching [`Op::Ret`] pops it and resumes.
    Call {
        func: u32,
    },
    /// Invoke syscall. `idx` is the index into [`Vm::syscalls`]. See
    /// [`crate::BuiltinFn`] for the syscall calling convention. It matches
    /// [`Op::Call`]: args in `r0..r{argcount-1}`, result written to `r0`.
    Sys {
        idx: u16,
    },
    /// Push `src` onto [`Vm::spilled`]. Used both for caller-save spill
    /// around [`Op::Call`] / [`Op::Sys`] and for cycle-breaking inside
    /// `bc::Cc::emit_arg_shuffle`.
    ///
    /// Invariant: every function's bytecode must leave [`Vm::spilled`] at
    /// the same depth on [`Op::Ret`] as on entry. Unbalanced push/pop pairs
    /// silently corrupt the caller's spilled values; the debug check on
    /// [`Op::Ret`] catches this in dev builds.
    Push {
        src: u8,
    },
    Push2 {
        a: u8,
        b: u8,
    },
    Push3 {
        a: u8,
        b: u8,
        c: u8,
    },
    /// Pop the top of [`Vm::spilled`] into `dst`. See [`Op::Push`] for the
    /// stack-balance invariant.
    Pop {
        dst: u8,
    },
    Pop2 {
        a: u8,
        b: u8,
    },
    Pop3 {
        a: u8,
        b: u8,
        c: u8,
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
    Alloc {
        dst: u8,
        kind: AllocType,
        size: u32,
        align: u8,
    },
    Store {
        base: u8,
        offset: u32,
        src: u8,
    },
    Load {
        dst: u8,
        base: u8,
        offset: u32,
    },
    Ret,
    Nop,
}

#[cfg(test)]
mod op_test {
    #[test]
    fn op_size_8_byte() {
        assert_eq!(std::mem::size_of::<crate::op::Op>(), 8);
    }
}
