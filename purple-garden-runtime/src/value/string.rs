use purple_garden_ir::ptype::Type;

use super::{FromVm, IntoVm, PgType, Value};
use crate::vm::Vm;

/// A garden `Str` as builtins handle it. Builtins only construct and read it, the representation
/// is private, so small strings or ropes can be added without touching them.
///
/// Today every `Str` is a string interned in the vm, passed through as the [`Value`] it already is:
/// neither copied out of the vm nor interned again on the way back.
pub struct Str(Repr);

enum Repr {
    Interned(Value),
}

impl Str {
    /// `s` interned in `vm`.
    pub fn new(vm: &mut Vm, s: &str) -> Self {
        Self(Repr::Interned(vm.new_string_from_str(s)))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        match &self.0 {
            Repr::Interned(value) => value.as_str(),
        }
    }
}

impl PgType for Str {
    const TYPE: Type<'static> = Type::Str;
}

impl FromVm<'_> for Str {
    fn from_vm(_: &Vm, value: Value) -> Self {
        Self(Repr::Interned(value))
    }
}

impl IntoVm for Str {
    fn into_vm(self, _: &mut Vm) -> Value {
        match self.0 {
            Repr::Interned(value) => value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Str;
    use crate::{FromVm, IntoVm, Vm, VmConfig};

    #[test]
    fn round_trips_the_interned_value() {
        let mut vm = Vm::new(VmConfig::default());
        let interned = vm.new_string_from_str("garden");

        let s = Str::new(&mut vm, "garden");
        assert_eq!(s.as_str(), "garden");
        assert_eq!(s.into_vm(&mut vm), interned);

        let s = Str::from_vm(&vm, interned);
        assert_eq!(s.into_vm(&mut vm), interned);
    }
}
