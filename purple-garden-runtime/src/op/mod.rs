// codegen for both packed and base op codes
macro_rules! ops {
    (
        // match a single definition
        $(
            // meta tags like doc comments, which are just #[doc]
            $(#[$meta:meta])*

            // definition and tag name
            $name:ident

            // body delimited with {}, matches field containers, is optional via ? to support Halt
            // and Nop (bodyless variants)
            $({ $($body:tt)* })?
        ),*
        $(,)?
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Op {
             $( $(#[$meta])* $name $({ $($body)* })? ),*
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u8)]
        pub enum Tag {
            $($name),*
        }

        impl Tag {
            pub const COUNT: usize = [$(Tag::$name),*].len();
        }

        /// static assertion since tag is 6 bits, meaning 2^6=64 op codes can be represented
        const _: () = assert!(Tag::COUNT <= 64);

        impl Op {
            pub const fn tag(&self) -> Tag {
                match self {
                    $(Self::$name {..} => Tag::$name),*
                }
            }
        }
    }
}

use crate::AllocType;

pub mod packed;

ops! {
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
        imm: i16,
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
        imm: i16,
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
        imm: i16,
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
        imm: i16,
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
        imm: i16,
    },
    IAnd {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    /// `r[dst] = r[lhs] & imm`. Lowered from IR `BinImm` (commutative).
    IAndI {
        dst: u8,
        lhs: u8,
        imm: i16,
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
        imm: i16,
    },
    /// `r[dst] = r[lhs] > imm`. Lowered from IR `BinImm`, including
    /// swapped constant-lhs comparisons.
    IGtI {
        dst: u8,
        lhs: u8,
        imm: i16,
    },
    /// `r[dst] = r[lhs] < imm`. Lowered from IR `BinImm`, including
    /// swapped constant-lhs comparisons.
    ILtI {
        dst: u8,
        lhs: u8,
        imm: i16,
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
    /// Int `<=` is `!(>)`, a NaN makes that wrong for doubles.
    DLe {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    DGe {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    BEq {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    /// `r[dst] = r[lhs] == r[rhs]` by string content, see [`Value::str_eq`](crate::Value::str_eq).
    SEq {
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
        imm: i8,
        target: u16,
    },
    /// Conditional jump: branch when `r[lhs] != imm`.
    JmpNeI {
        lhs: u8,
        imm: i8,
        target: u16,
    },
    /// Conditional jump: branch when `r[lhs] == r[rhs]` by string content.
    JmpSEq {
        lhs: u8,
        rhs: u8,
        target: u16,
    },
    /// Conditional jump: branch when `r[lhs] != r[rhs]` by string content.
    JmpSNe {
        lhs: u8,
        rhs: u8,
        target: u16,
    },
    /// Jump table on `r[subject]`: jump to `table.targets[slot]` for
    /// `slot = r[subject] - table.first`, scaled per [`SwitchKind`]. Falls
    /// through to the next op when `slot` is out of range.
    Switch {
        kind: SwitchKind,
        subject: u8,
        /// Index into [`crate::vm::Vm::switch_tables`].
        table: u16,
    },
    /// `r[dst] = table.values[slot]`, slot as for [`Op::Switch`];
    /// `table.default` when `slot` is out of range.
    Lookup {
        kind: SwitchKind,
        dst: u8,
        subject: u8,
        /// Index into [`crate::vm::Vm::lookup_tables`].
        table: u16,
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
    /// Call builtin `idx` with its `argc` arguments in `r0..argc`; it may
    /// write `r0..max(argc, 1)`, the registers the caller spilled.
    Sys {
        idx: u16,
        argc: u8,
    },
    /// Enter the native function at [`Vm::syscalls`] index `idx` through
    /// [`crate::enter_native`], or [`crate::enter_native_double`] if it returns a Double. A trap
    /// inside it leaves through [`crate::jit_abort`] back to here.
    Native {
        idx: u16,
        returns_double: bool,
    },
    /// Push `src` onto [`Vm::spilled`].
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
    /// Overwrite the value `slot` below the top of [`Vm::spilled`] with `src`. Slots count from
    /// the top, so `slot` includes everything pushed after the slot was reserved.
    StoreSlot {
        slot: u16,
        src: u8,
    },
    /// Read the value `slot` below the top of [`Vm::spilled`] into `dst`, see [`Op::StoreSlot`].
    LoadSlot {
        dst: u8,
        slot: u16,
    },

    CastToInt {
        dst: u8,
        src: u8,
    },
    CastToDouble {
        dst: u8,
        src: u8,
    },
    CastToBool {
        dst: u8,
        src: u8,
    },
    Alloc {
        dst: u8,
        kind: AllocType,
        size: u16,
        align: u8,
    },
    Store {
        base: u8,
        offset: u16,
        src: u8,
    },
    Load {
        dst: u8,
        base: u8,
        offset: u16,
    },
    AddrOf {
        dst: u8,
        base: u8,
        offset: u16,
    },
    Ret,
    /// Trap with [`crate::Anomaly::AssertionFailed`] when `cond` is false.
    Assert {
        cond: u8,
    },
    /// Stop the dispatch loop
    Halt,
    Nop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SwitchKind {
    /// Interned string keys in the string pool, `first` is the lowest key's
    /// address and a slot spans [`crate::string::ALIGN`] bytes.
    Str = 0,
    /// Integer keys, `first` is the smallest.
    Int = 1,
}

/// The value table of an [`Op::Lookup`], owned by [`crate::vm::Vm::lookup_tables`].
#[derive(Debug, Clone)]
pub struct LookupTable {
    pub first: u64,
    /// A value per slot, slots without a key hold `default`.
    pub values: Box<[crate::Value]>,
    pub default: crate::Value,
}

/// The jump table of an [`Op::Switch`], owned by [`crate::vm::Vm::switch_tables`]
/// so the op can stay `Copy`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchTable {
    pub first: u64,
    /// A pc per slot, slots without a key hold the default's pc.
    pub targets: Box<[u16]>,
}

#[cfg(test)]
mod op_test {
    #[test]
    fn op_size_8_byte() {
        assert_eq!(std::mem::size_of::<crate::op::Op>(), 8);
    }
}
