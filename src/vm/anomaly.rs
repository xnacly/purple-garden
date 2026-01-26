/// Anomaly is a user error bubbling up in the virtual machine
#[derive(Debug)]
pub enum Anomaly {
    DivisionByZero { pc: usize },
    Unimplemented { pc: usize },
}

impl Anomaly {
    pub fn as_str(&self) -> &str {
        match self {
            Anomaly::DivisionByZero { .. } => "Division by zero",
            Anomaly::Unimplemented { .. } => "Unimplemented",
        }
    }
}
