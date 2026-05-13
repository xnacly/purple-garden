/// Anomaly is a user error bubbling up in the virtual machine
#[derive(Debug)]
pub enum Anomaly {
    DivisionByZero { pc: usize, span: u32 },
    InvalidSyscall { pc: usize, span: u32 },
    Msg { msg: &'static str, pc: usize, span: u32 },
}

impl Anomaly {
    pub fn as_str(&self) -> &str {
        match self {
            Anomaly::DivisionByZero { .. } => "Division by zero",
            Anomaly::InvalidSyscall { .. } => "InvalidSyscall",
            Anomaly::Msg { msg, .. } => msg,
        }
    }

    /// Source byte offset of the originating AST node; threaded by
    /// `bc::Cc::pc_to_span` and stamped onto the anomaly at raise time.
    pub fn span(&self) -> u32 {
        match self {
            Anomaly::DivisionByZero { span, .. }
            | Anomaly::InvalidSyscall { span, .. }
            | Anomaly::Msg { span, .. } => *span,
        }
    }
}
