use purple_garden_ir::ptype::Type;

use super::{FromVm, IntoVm, Value};
use crate::{AllocType, vm::Vm};

#[doc(hidden)]
pub fn alloc_record(vm: &mut Vm, ty: &Type<'static>) -> Value {
    let Type::Record(_) = ty else {
        panic!("record allocation requires record type, got {ty}");
    };
    Value::from_ptr(vm.alloc(AllocType::Record, ty.layout()).as_ptr())
}

#[doc(hidden)]
pub fn record_field_ptr(base: Value, record_ty: &Type<'static>, name: &str) -> *mut u8 {
    let offset = record_ty
        .field_offset(name)
        .unwrap_or_else(|| panic!("record type {record_ty} has no field `{name}`"));
    unsafe { base.as_ptr::<u8>().add(offset) }
}

#[doc(hidden)]
pub fn record_field_ty<'a>(record_ty: &'a Type<'static>, name: &str) -> &'a Type<'static> {
    let Type::Record(fields) = record_ty else {
        panic!("field lookup requires record type, got {record_ty}");
    };
    &fields
        .iter()
        .find(|field| field.name == name)
        .unwrap_or_else(|| panic!("record type {record_ty} has no field `{name}`"))
        .ty
}

#[doc(hidden)]
pub unsafe fn decode_record_field<'vm, T: FromVm<'vm>>(
    vm: &'vm Vm,
    base: Value,
    record_ty: &Type<'static>,
    name: &str,
) -> T {
    let src = record_field_ptr(base, record_ty, name).cast_const();
    let field_ty = record_field_ty(record_ty, name);
    let value = match field_ty {
        Type::Record(_) => Value::from_ptr(src.cast_mut()),
        _ => unsafe { read_value(src) },
    };
    T::from_vm(vm, value)
}

#[doc(hidden)]
pub fn encode_record_field<T: IntoVm>(
    vm: &mut Vm,
    base: Value,
    record_ty: &Type<'static>,
    name: &str,
    value: T,
) {
    let dst = record_field_ptr(base, record_ty, name);
    let field_ty = record_field_ty(record_ty, name);
    let value = value.into_vm(vm);
    match field_ty {
        Type::Record(_) => unsafe { copy_record(value, dst, field_ty) },
        _ => write_value(dst, value),
    }
}

#[doc(hidden)]
pub unsafe fn copy_record(record: Value, dst: *mut u8, ty: &Type<'static>) {
    unsafe {
        std::ptr::copy_nonoverlapping(record.as_ptr::<u8>(), dst, ty.size());
    }
}

unsafe fn read_value(src: *const u8) -> Value {
    unsafe { src.cast::<Value>().read_unaligned() }
}

fn write_value(dst: *mut u8, value: Value) {
    unsafe { dst.cast::<Value>().write_unaligned(value) };
}
