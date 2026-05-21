/// Anomaly is a user error bubbling up in the virtual machine.
///
/// Variants carry the trap `pc` only. Source-location resolution
/// happens at error-rendering time via `bc::DebugInfo::span_at(pc)`,
/// keeping the runtime hot path free of source-info bookkeeping.
#[derive(Debug)]
pub enum Anomaly {
    DivisionByZero { pc: usize },
    InvalidSyscall { pc: usize },
    Msg { msg: &'static str, pc: usize },
}

impl Anomaly {
    pub fn as_str(&self) -> &str {
        match self {
            Anomaly::DivisionByZero { .. } => "Division by zero",
            Anomaly::InvalidSyscall { .. } => "InvalidSyscall",
            Anomaly::Msg { msg, .. } => msg,
        }
    }

    /// PC at which the trap fired. Pair with `bc::DebugInfo::span_at`
    /// to recover the source byte offset.
    pub fn pc(&self) -> usize {
        match self {
            Anomaly::DivisionByZero { pc }
            | Anomaly::InvalidSyscall { pc }
            | Anomaly::Msg { pc, .. } => *pc,
        }
    }
}
