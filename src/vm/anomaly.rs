/// Anomaly is a user error bubbling up in the virtual machine
#[derive(Debug)]
pub enum Anomaly {
    DivisionByZero { pc: usize },
    UndefinedLocal { pc: usize },
    Unimplemented { pc: usize },
    // TODO: this needs some kind of data for expected types, maybe hardcoded &'static str?
    TypeIncompatible { pc: usize },
}

impl Anomaly {
    pub fn as_str(&self) -> &str {
        match self {
            Anomaly::DivisionByZero { .. } => "Division by zero",
            Anomaly::UndefinedLocal { .. } => "Undefined Local",
            Anomaly::Unimplemented { .. } => "Unimplemented",
            Anomaly::TypeIncompatible { .. } => "Type Incompatible",
        }
    }
}
