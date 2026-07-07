/// Anomaly is a user error bubbling up in the virtual machine.
///
/// Variants carry the trap `pc` only. Source-location resolution
/// happens at error-rendering time via `Program::debug`.
#[derive(Debug)]
pub enum Anomaly {
    DivisionByZero { pc: usize },
    InvalidSyscall { pc: usize },
    AllocationFailed { pc: usize },
    Msg { msg: &'static str, pc: usize },
}

impl Anomaly {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Anomaly::DivisionByZero { .. } => "Division by zero",
            Anomaly::InvalidSyscall { .. } => "InvalidSyscall",
            Anomaly::AllocationFailed { .. } => "Allocation failed",
            Anomaly::Msg { msg, .. } => msg,
        }
    }

    /// PC at which the trap fired. Pair with `Program::debug.span_at`
    /// to recover the source byte offset.
    #[must_use]
    pub fn pc(&self) -> usize {
        match self {
            Anomaly::DivisionByZero { pc }
            | Anomaly::InvalidSyscall { pc }
            | Anomaly::AllocationFailed { pc }
            | Anomaly::Msg { pc, .. } => *pc,
        }
    }
}
