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

/// # Safety
///
/// `base` is a live record of `record_ty`.
#[doc(hidden)]
pub unsafe fn record_field_ptr(base: Value, record_ty: &Type<'static>, name: &str) -> *mut u8 {
    let offset = record_ty
        .field_offset(name)
        .unwrap_or_else(|| panic!("record type {record_ty} has no field `{name}`"));
    // SAFETY: caller upholds # Safety, field_offset is in bounds
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

/// # Safety
///
/// `base` is a live record of `record_ty`.
#[doc(hidden)]
pub unsafe fn decode_record_field<'vm, T: FromVm<'vm>>(
    vm: &'vm Vm,
    base: Value,
    record_ty: &Type<'static>,
    name: &str,
) -> T {
    // SAFETY: caller upholds # Safety
    let src = unsafe { record_field_ptr(base, record_ty, name) }.cast_const();
    let field_ty = record_field_ty(record_ty, name);
    let value = match field_ty {
        Type::Record(_) => Value::from_ptr(src.cast_mut()),
        // SAFETY: src is a non-record field slot of a live record
        _ => unsafe { read_value(src) },
    };
    T::from_vm(vm, value)
}

/// # Safety
///
/// `base` is a live record of `record_ty`.
#[doc(hidden)]
pub unsafe fn encode_record_field<T: IntoVm>(
    vm: &mut Vm,
    base: Value,
    record_ty: &Type<'static>,
    name: &str,
    value: T,
) {
    // SAFETY: caller upholds # Safety
    let dst = unsafe { record_field_ptr(base, record_ty, name) };
    let field_ty = record_field_ty(record_ty, name);
    let value = value.into_vm(vm);
    match field_ty {
        // SAFETY: value is a record of field_ty, dst its inline slot
        Type::Record(_) => unsafe { copy_record(value, dst, field_ty) },
        // SAFETY: dst is a non-record field slot of a live record
        _ => unsafe { write_value(dst, value) },
    }
}

/// # Safety
///
/// `record` is a record of `ty`, `dst` is valid for `ty.size()` bytes of writes.
#[doc(hidden)]
pub unsafe fn copy_record(record: Value, dst: *mut u8, ty: &Type<'static>) {
    // SAFETY: caller passes a record of ty and room for it at dst
    unsafe {
        std::ptr::copy_nonoverlapping(record.as_ptr::<u8>(), dst, ty.size());
    }
}

unsafe fn read_value(src: *const u8) -> Value {
    // SAFETY: caller passes 8 readable bytes
    unsafe { src.cast::<Value>().read_unaligned() }
}

unsafe fn write_value(dst: *mut u8, value: Value) {
    // SAFETY: caller passes 8 writable bytes
    unsafe { dst.cast::<Value>().write_unaligned(value) };
}
