use std::fmt::Debug;

use purple_garden_ir::{constant::Const, ptype::Type};

use crate::vm::Vm;

/// Raw VM word.
///
/// `Value` is the VM-internal scalar encoding. Its `From` impls are intentionally
/// limited to context-free values; embedding/syscall boundaries should use
/// [`FromVm`] and [`IntoVm`] instead.
#[derive(PartialEq, Clone, Default, Copy, Debug)]
#[repr(transparent)]
pub struct Value(pub u64);

impl Value {
    pub const UNDEF: Self = Self(0);

    #[inline(always)]
    #[must_use]
    pub fn as_int(&self) -> i64 {
        self.0 as i64
    }

    #[inline(always)]
    #[must_use]
    pub fn as_bool(&self) -> bool {
        self.0 != 0
    }

    #[inline(always)]
    #[must_use]
    pub fn as_f64(&self) -> f64 {
        f64::from_bits(self.0)
    }

    #[inline(always)]
    #[must_use]
    pub fn as_str<'t>(&self, pool: &'t [(u32, u32)], data: &'t str) -> &'t str {
        let (off, len) = pool[self.0 as usize];
        &data[off as usize..off as usize + len as usize]
    }

    #[inline(always)]
    #[must_use]
    pub fn as_ptr<T>(&self) -> *mut T {
        self.0 as *mut T
    }

    #[inline(always)]
    pub fn from_ptr<T>(ptr: *mut T) -> Self {
        Self(ptr as u64)
    }

    #[inline(always)]
    #[must_use]
    pub fn int_to_bool(&self) -> Self {
        Value::from(self.0 != 0)
    }

    #[inline(always)]
    #[must_use]
    pub fn int_to_f64(&self) -> Self {
        Value::from(self.as_int() as f64)
    }

    #[inline(always)]
    #[must_use]
    pub fn f64_to_int(&self) -> Self {
        Value::from(self.as_f64() as i64)
    }
}

impl<'c> From<Const<'c>> for Value {
    fn from(value: Const<'c>) -> Self {
        Self(match value {
            Const::False => 0u64,
            Const::True => 1u64,
            Const::Int(i) => i as u64,
            Const::Double(bits) => bits,
            _ => unimplemented!("0xDEADAFFE"),
        })
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self(u64::from(value))
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self(value as u64)
    }
}

impl From<usize> for Value {
    fn from(value: usize) -> Self {
        Self(value as u64)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Self(value.to_bits())
    }
}

pub trait PgType {
    const TYPE: Type;
}

/// Decode a Rust value from the VM argument registers.
pub trait FromVm<'vm>: Sized {
    fn from_vm(vm: &'vm Vm, idx: usize) -> Self;
}

/// Encode a Rust return value into a raw VM value.
///
/// The VM is available for return types that need runtime allocation, such as
/// strings. The caller still decides where the returned [`Value`] is written.
pub trait IntoVm {
    fn into_vm(self, vm: &mut Vm) -> Value;
}

impl PgType for &str {
    const TYPE: Type = Type::Str;
}

impl<'vm> FromVm<'vm> for &'vm str {
    fn from_vm(vm: &'vm Vm, idx: usize) -> Self {
        vm.r(idx).as_str(vm.strings(), vm.string_data())
    }
}

impl IntoVm for &str {
    fn into_vm(self, vm: &mut Vm) -> Value {
        let idx = vm.new_string(self.to_owned());
        Value::from(idx)
    }
}

impl PgType for String {
    const TYPE: Type = Type::Str;
}

impl<'vm> FromVm<'vm> for String {
    fn from_vm(vm: &'vm Vm, idx: usize) -> Self {
        vm.r(idx).as_str(vm.strings(), vm.string_data()).to_owned()
    }
}

impl IntoVm for String {
    fn into_vm(self, vm: &mut Vm) -> Value {
        let idx = vm.new_string(self);
        Value::from(idx)
    }
}

impl PgType for i64 {
    const TYPE: Type = Type::Int;
}

impl<'vm> FromVm<'vm> for i64 {
    fn from_vm(vm: &'vm Vm, idx: usize) -> Self {
        vm.r(idx).as_int()
    }
}

impl IntoVm for i64 {
    fn into_vm(self, _: &mut Vm) -> Value {
        Value::from(self)
    }
}

impl PgType for f64 {
    const TYPE: Type = Type::Double;
}

impl<'vm> FromVm<'vm> for f64 {
    fn from_vm(vm: &'vm Vm, idx: usize) -> Self {
        vm.r(idx).as_f64()
    }
}

impl IntoVm for f64 {
    fn into_vm(self, _: &mut Vm) -> Value {
        Value::from(self)
    }
}

impl PgType for bool {
    const TYPE: Type = Type::Bool;
}

impl<'vm> FromVm<'vm> for bool {
    fn from_vm(vm: &'vm Vm, idx: usize) -> Self {
        vm.r(idx).as_bool()
    }
}

impl IntoVm for bool {
    fn into_vm(self, _: &mut Vm) -> Value {
        Value::from(self)
    }
}

impl PgType for () {
    const TYPE: Type = Type::Void;
}

impl<'vm> FromVm<'vm> for () {
    fn from_vm(_: &'vm Vm, _: usize) -> Self {}
}

impl IntoVm for () {
    fn into_vm(self, _: &mut Vm) -> Value {
        Value::UNDEF
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VmConfig;

    #[test]
    fn vm_conversion_traits_roundtrip_primitives() {
        let mut vm = Vm::new(VmConfig::default());
        let hello = vm.new_string("hello".to_owned());
        let world = vm.new_string("world".to_owned());
        *vm.r_mut(0) = Value::from(hello);
        *vm.r_mut(1) = Value::from(world);
        *vm.r_mut(2) = Value::from(42_i64);
        *vm.r_mut(3) = Value::from(3.5_f64);
        *vm.r_mut(4) = Value::from(true);

        let arg0 = <&str as FromVm>::from_vm(&vm, 0);
        let arg1 = <&str as FromVm>::from_vm(&vm, 1);
        assert_eq!((arg0, arg1), ("hello", "world"));
        assert_eq!(<i64 as FromVm>::from_vm(&vm, 2), 42);
        assert_eq!(<f64 as FromVm>::from_vm(&vm, 3), 3.5);
        assert!(<bool as FromVm>::from_vm(&vm, 4));
        assert_eq!(<String as FromVm>::from_vm(&vm, 0), "hello");

        *vm.r_mut(0) = "returned".into_vm(&mut vm);
        assert_eq!(vm.r(0).as_str(vm.strings(), vm.string_data()), "returned");
        *vm.r_mut(0) = 7_i64.into_vm(&mut vm);
        assert_eq!(vm.r(0).as_int(), 7);
        *vm.r_mut(0) = 2.25_f64.into_vm(&mut vm);
        assert_eq!(vm.r(0).as_f64(), 2.25);
        *vm.r_mut(0) = false.into_vm(&mut vm);
        assert!(!vm.r(0).as_bool());
        assert_eq!(().into_vm(&mut vm), Value::UNDEF);
    }
}
