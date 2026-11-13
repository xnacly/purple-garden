//! What an [`crate::inspect`] records, owned and serializable so it can leave the process that
//! compiled it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Inspection {
    /// Parse and type errors, the pipeline stops after the frontend if there are any.
    pub diagnostics: Vec<Diagnostic>,
    /// The `-A` dump.
    pub ast: String,
    /// The `-T` dump: top level bindings and function signatures.
    pub types: String,
    /// The `-TT` dump: every value of the AST with its type.
    pub typed_ast: String,
    /// Every IR function, callees before their callers.
    pub functions: Vec<Function>,
    /// The constants of the bytecode, by slot.
    pub globals: Vec<String>,
    /// The `-D` listing.
    pub bytecode: Vec<BcLine>,
    /// Time and memory every phase took, in pipeline order.
    pub phases: Vec<Phase>,
    /// The error lowering or compiling stopped with.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Byte offset into the source.
    pub start: usize,
    /// Byte length, `0` for a point.
    pub len: usize,
    pub message: String,
    /// The terminal rendering, with the source excerpt, notes and helps.
    pub rendered: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phase {
    pub name: String,
    pub micros: u64,
    pub allocs: usize,
    pub frees: usize,
    pub bytes: usize,
    pub peak: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Function {
    pub id: u32,
    pub name: String,
    /// Byte offset of the definition into the source.
    pub span: u32,
    /// Not reachable from the entry, so neither lowered to bytecode nor compiled natively.
    pub unused: bool,
    /// The IR as lowered, then after every pass that ran on it.
    pub snapshots: Vec<Snapshot>,
    /// `intervals[value]` is the `(def, last use)` row range of `value` in the final IR, `None`
    /// for values defined nowhere.
    pub intervals: Vec<Option<(u32, u32)>>,
    /// Where the bytecode keeps every value, empty if the function wasn't lowered to bytecode.
    pub bc_locations: Vec<Option<Location>>,
    /// The function compiled by the x86-64 backend, or why it isn't.
    pub x86: Result<Native, String>,
}

impl Function {
    /// The IR after `snapshot`, unchanged snapshots fall back to the last one that changed it.
    #[must_use]
    pub fn ir_at(&self, snapshot: usize) -> &Ir {
        self.snapshots[..=snapshot.min(self.snapshots.len() - 1)]
            .iter()
            .rev()
            .find_map(|s| s.ir.as_ref())
            .expect("the first snapshot always holds the IR")
    }

    /// The IR every later stage was compiled from.
    #[must_use]
    pub fn final_ir(&self) -> &Ir {
        self.ir_at(self.snapshots.len() - 1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// The pass that ran, `lower` for the IR before any pass.
    pub pass: String,
    /// `None` if the pass left the function as it was.
    pub ir: Option<Ir>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ir {
    pub rows: Vec<IrRow>,
    /// The live blocks, in layout order.
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrRow {
    /// The liveness row, shared with [`Function::intervals`], [`BcLine::row`] and
    /// [`AsmLine::row`].
    pub pos: Option<u32>,
    pub indent: u8,
    pub text: String,
    pub block: Option<u32>,
    /// Byte offset into the source.
    pub span: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub id: u32,
    /// The blocks its terminator jumps to.
    pub succs: Vec<u32>,
    /// The functions it calls or tail calls, by id, in order.
    pub calls: Vec<u32>,
    /// Indices into [`Ir::rows`] of the block's label and its last row.
    pub rows: (usize, usize),
}

/// Where a value lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Location {
    /// A register, by its name in the target: `r3` in the bytecode, `rbx` or `xmm1` on x86-64.
    Reg(String),
    /// A stack slot.
    Slot(u16),
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Location::Reg(name) => f.write_str(name),
            Location::Slot(slot) => write!(f, "s{slot}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BcLineKind {
    Func,
    Block,
    Op,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BcLine {
    pub kind: BcLineKind,
    pub pc: usize,
    pub text: String,
    /// The function the line is part of.
    pub func: Option<u32>,
    /// Byte offset into the source of an op.
    pub span: Option<u32>,
    /// The liveness row of the IR an op was lowered from.
    pub row: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Native {
    /// Address the function was laid out at, functions follow each other like in the code arena.
    pub address: u64,
    pub size: usize,
    pub lines: Vec<AsmLine>,
    /// Where the machine code keeps every value.
    pub locations: Vec<Option<Location>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsmLine {
    /// Offset from the function's first byte.
    pub offset: usize,
    /// Hex of the instruction's bytes.
    pub bytes: String,
    /// Intel syntax, or the bytes of an inlined table.
    pub text: String,
    /// A block or function label in front of the line.
    pub label: Option<String>,
    /// Bytes of an inlined jump or lookup table, not an instruction.
    pub data: bool,
    /// The liveness row of the IR the instruction was lowered from, `None` for the frame.
    pub row: Option<u32>,
}
