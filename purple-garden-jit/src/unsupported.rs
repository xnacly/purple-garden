use std::fmt;

use purple_garden_ir::{self as ir, BinOp, UnOp, ptype::Type};
use purple_garden_shared::xralloc2::Reg;

/// Why a function stays bytecode. Building one only stores what its message needs, the text is
/// formatted by [`fmt::Display`] alone, so it costs nothing where no reason is printed.
#[derive(Debug, Clone)]
pub enum Unsupported<'ir> {
    /// The aarch64 backend doesn't compile anything yet.
    Scaffold,
    EmptyFunction,
    /// A value of the wrong register class reached an instruction.
    NotGpr,
    NotFpr,
    ParamRegisterTaken(Reg),
    NoRegister(ir::Id),
    /// An instruction reads more spilled values than there are temps to load them into.
    OutOfTemps(ir::Id),
    UseBeforeDefinition(ir::Id),
    AllocationType,
    /// The string constant, lookup string or switch key isn't in the const pool.
    MissingConstant(&'static str),
    ConstNotBoolOrI32,
    BinImmOp(BinOp),
    BinOp(BinOp),
    NeedsAvx,
    NeedsBmi2(BinOp),
    MissingCpuFeature(UnOp),
    Cast {
        from: Type<'ir>,
        to: Type<'ir>,
    },
    /// A lookup or switch table with more slots than an i32 holds.
    TableSize {
        table: &'static str,
        len: usize,
    },
    NoNativeEntry(&'ir str),
    NotCompiledNatively(ir::Id),
    TooManyArgs(usize),
    Instruction(ir::Instr<'ir>),
    Terminator,
    /// A jump, edge move or switch pad further away than a rel32 reaches.
    OutOfRel32(&'static str),
}

impl fmt::Display for Unsupported<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scaffold => write!(f, "the aarch64 backend is a scaffold only"),
            Self::EmptyFunction => write!(f, "empty function"),
            Self::NotGpr => write!(f, "gpr called on a value thats not a gpr"),
            Self::NotFpr => write!(f, "xmm called on a value thats not a fpr"),
            Self::ParamRegisterTaken(reg) => write!(f, "param register {reg:?} is taken"),
            Self::NoRegister(id) => write!(f, "%v{} has no register", id.0),
            Self::OutOfTemps(id) => write!(f, "more operands in slots than temps at %v{}", id.0),
            Self::UseBeforeDefinition(id) => write!(f, "use of %v{} before its definition", id.0),
            Self::AllocationType => write!(f, "unsupported allocation type"),
            Self::MissingConstant(what) => write!(f, "{what} missing from the const pool"),
            Self::ConstNotBoolOrI32 => write!(f, "const is not a bool or i32"),
            Self::BinImmOp(op) => write!(f, "unsupported binimm op {op:?}"),
            Self::BinOp(op) => write!(f, "unsupported bin op {op:?}"),
            Self::NeedsAvx => write!(
                f,
                "double bin ops need avx, your shitty ass system is too old :^)"
            ),
            Self::NeedsBmi2(op) => write!(f, "{op:?} needs bmi2"),
            Self::MissingCpuFeature(op) => write!(f, "{op:?} is missing its cpu feature"),
            Self::Cast { from, to } => write!(f, "unsupported cast from {from} to {to}"),
            Self::TableSize { table, len } => write!(f, "{table} table of {len} slots"),
            Self::NoNativeEntry(name) => write!(f, "{name} has no native entry"),
            Self::NotCompiledNatively(id) => write!(f, "f{} is not compiled natively", id.0),
            Self::TooManyArgs(n) => write!(f, "{n} args, more than argument registers"),
            Self::Instruction(i) => write!(f, "unsupported instruction {i:?}"),
            Self::Terminator => write!(f, "unsupported terminator"),
            Self::OutOfRel32(what) => write!(f, "{what} out of rel32 range"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Unsupported;
    use purple_garden_ir::{BinOp, Id, ptype::Type};

    #[test]
    fn reasons_read_as_they_did_as_strings() {
        let cases: [(Unsupported<'static>, &str); 6] = [
            (Unsupported::EmptyFunction, "empty function"),
            (
                Unsupported::NotCompiledNatively(Id(3)),
                "f3 is not compiled natively",
            ),
            (Unsupported::NeedsBmi2(BinOp::IShl), "IShl needs bmi2"),
            (
                Unsupported::Cast {
                    from: Type::Bool,
                    to: Type::Double,
                },
                "unsupported cast from Bool to Double",
            ),
            (
                Unsupported::TableSize {
                    table: "switch",
                    len: 1 << 40,
                },
                "switch table of 1099511627776 slots",
            ),
            (
                Unsupported::OutOfRel32("edge moves"),
                "edge moves out of rel32 range",
            ),
        ];
        for (reason, text) in cases {
            assert_eq!(reason.to_string(), text);
        }
    }
}
