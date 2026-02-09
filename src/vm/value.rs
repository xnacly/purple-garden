use crate::ir::Const;

#[derive(Debug, PartialEq, Clone, Default)]
pub enum Value<'v> {
    /// An invalid value, not exposed to the user, no way for the user to create this
    #[default]
    UnDef,
    True,
    False,
    Int(i64),
    // TODO: see https://research.swtch.com/fp (Floating-Point Printing and Parsing Can Be Simple)
    Double(f64),
    /// a view into the bytes of the interpreters input, compile time strings
    Str(&'v str),
    /// a dynamic string with owned memory, heap allocated
    String(String),
    // Arr(Gc<[Value<'v>]>),
    // Obj(Gc<Todo>),
}

impl Value<'_> {
    #[inline(always)]
    pub fn as_int(&self) -> i64 {
        unsafe {
            match self {
                Value::Int(v) => *v,
                _ => std::hint::unreachable_unchecked(),
            }
        }
    }

    #[inline(always)]
    pub fn as_bool(&self) -> bool {
        unsafe {
            match self {
                Value::True => true,
                Value::False => false,
                _ => std::hint::unreachable_unchecked(),
            }
        }
    }
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

impl From<bool> for Value<'_> {
    fn from(value: bool) -> Self {
        if value { Value::True } else { Value::False }
    }
}

impl From<i64> for Value<'_> {
    fn from(value: i64) -> Self {
        Value::Int(value)
    }
}

impl From<f64> for Value<'_> {
    fn from(value: f64) -> Self {
        Value::Double(value)
    }
}

impl<'s> From<&'s str> for Value<'s> {
    fn from(value: &'s str) -> Self {
        Value::Str(value)
    }
}
