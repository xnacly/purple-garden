use purple_garden_ir::ptype::Type;

use super::{FromVm, IntoVm, PgType, Value};
use crate::vm::Vm;

/// A `Str` passed through as the interned [`Value`] it already is, neither copied out of the vm
/// nor interned again on the way back.
pub struct RawString(pub Value);

impl PgType for RawString {
    const TYPE: Type<'static> = Type::Str;
}

impl FromVm<'_> for RawString {
    fn from_vm(_: &Vm, value: Value) -> Self {
        Self(value)
    }
}

impl IntoVm for RawString {
    fn into_vm(self, _: &mut Vm) -> Value {
        self.0
    }
}
