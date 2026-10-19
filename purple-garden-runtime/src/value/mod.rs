use std::fmt::Debug;

use purple_garden_ir::constant::Const;

use crate::string;

mod conversion;
mod record;
mod stringview;

pub use conversion::{FromVm, IntoVm, PgType, Slot};
pub use record::{
    alloc_record, copy_record, decode_record_field, encode_record_field, record_field_ptr,
    record_field_ty,
};
pub use stringview::RawString;

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
    pub fn as_str<'t>(&self) -> &'t str {
        let ptr = self.as_ptr::<u8>();
        let len = unsafe { (*(ptr as *const string::StrHeader)).len } as usize;
        let bytes = unsafe { std::slice::from_raw_parts(ptr.add(string::HEADER_SIZE), len) };
        unsafe { std::str::from_utf8_unchecked(bytes) }
    }

    #[inline(always)]
    #[must_use]
    pub fn str_hash(&self) -> u32 {
        unsafe { (*self.as_ptr::<string::StrHeader>()).hash }
    }

    /// Strings are interned, see [`string::StrTable`], so equal contents
    /// share one payload.
    #[inline(always)]
    #[must_use]
    pub fn str_eq(&self, other: &Value) -> bool {
        self.0 == other.0
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
            Const::Undefined => Value::UNDEF.0,
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
