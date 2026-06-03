use std::borrow::Cow;

/// Compile time Value representation, used for interning and constant propagation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum Const<'c> {
    #[default]
    Undefined,
    False,
    True,
    Int(i64),
    Double(u64),
    Str(Cow<'c, str>),
}

impl From<bool> for Const<'_> {
    fn from(value: bool) -> Self {
        if value { Const::True } else { Const::False }
    }
}

impl From<i64> for Const<'_> {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<f64> for Const<'_> {
    fn from(value: f64) -> Self {
        Self::Double(value.to_bits())
    }
}

impl<'c> From<&'c str> for Const<'c> {
    fn from(value: &'c str) -> Self {
        Self::Str(Cow::Borrowed(value))
    }
}

impl From<String> for Const<'_> {
    fn from(value: String) -> Self {
        Self::Str(Cow::Owned(value))
    }
}
