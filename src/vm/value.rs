use crate::cc::Const;

#[derive(Debug, PartialEq, Clone)]
pub enum Value<'v> {
    /// An invalid value, not exposed to the user, no way for the user to create this
    UnDef,
    True,
    False,
    Int(i64),
    Double(f64),
    /// a view into the bytes of the interpreters input, compile time strings
    Str(&'v str),
    /// a dynamic string with owned memory, heap allocated
    String(String),
    // Arr(Gc<[Value<'v>]>),
    // Obj(Gc<Todo>),
}

impl<'c> From<Const<'c>> for Value<'c> {
    fn from(value: Const<'c>) -> Self {
        match value {
            Const::False => Value::False,
            Const::True => Value::True,
            Const::Int(i) => Value::Int(i),
            Const::Double(bits) => Value::Double(f64::from_bits(bits)),
            Const::Str(str) => Value::Str(str),
        }
    }
}
