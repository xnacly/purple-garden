/// Anomaly is a user error bubbling up in the virtual machine
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
}
