use std::{
    fmt::{self, Debug, Display},
    hint::unreachable_unchecked,
};

use crate::ir::{Const, ptype};

/// Immutable string representation used in the purple garden NaN boxing value representation
#[repr(C)]
pub struct Str {
    pub ptr: *const u8,
    pub len: usize,
}

impl Str {
    #[inline(always)]
    pub fn as_str<'t>(&self) -> &'t str {
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.ptr, self.len)) }
    }

    #[inline(always)]
    pub fn from_str(s: &str) -> Self {
        Str {
            ptr: s.as_ptr(),
            len: s.len(),
        }
    }
}

#[derive(PartialEq, Clone, Default, Copy, Debug)]
#[repr(transparent)]
pub struct Value(pub u64);

impl Value {
    #[inline(always)]
    pub fn as_int(&self) -> i64 {
        debug_assert!(self.0 < i64::MAX as u64);
        self.0 as i64
    }

    #[inline(always)]
    pub fn as_bool(&self) -> bool {
        self.0 != 0
    }

    #[inline(always)]
    pub fn as_f64(&self) -> f64 {
        f64::from_bits(self.0)
    }

    #[inline(always)]
    pub fn as_str<'t>(&self) -> &'t str {
        let wrapper = self.as_ptr::<Str>();
        unsafe { (*wrapper).as_str() }
    }

    #[inline(always)]
    pub fn as_ptr<T>(&self) -> *mut T {
        { self.0 as *mut T }
    }

    #[inline(always)]
    pub fn from_ptr<T>(ptr: *mut T) -> Self {
        Self(ptr as u64)
    }

    #[inline(always)]
    pub fn int_to_bool(&self) -> Self {
        Value::from(self.0 != 0)
    }

    #[inline(always)]
    pub fn int_to_f64(&self) -> Self {
        Value::from(self.as_int() as f64)
    }

    #[inline(always)]
    pub fn f64_to_int(&self) -> Self {
        Value::from(self.as_f64() as i64)
    }

    pub fn dbg(&self, in_form_of: ptype::Type) -> String {
        match in_form_of {
            ptype::Type::Void => String::new(),
            ptype::Type::Bool => format!("{}", self.as_bool()),
            ptype::Type::Int => format!("{}", self.as_int()),
            ptype::Type::Double => format!("{}", self.as_f64()),
            ptype::Type::Str => format!("{:?}", self.as_str()),
            _ => todo!(),
        }
    }
}

impl<'c> From<Const<'c>> for Value {
    fn from(value: Const<'c>) -> Self {
        Self(match value {
            Const::False => 0u64,
            Const::True => 1u64,
            Const::Int(i) => i as u64,
            Const::Double(bits) => bits,
            Const::Str(str) => {
                let as_pg_str = &Str::from_str(str);
                return Value::from_ptr(as_pg_str as *const _ as *mut u8);
            }
        })
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self(if value { 1 } else { 0 })
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self(value as u64)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Self(value.to_bits())
    }
}

impl<'s> From<&'s str> for Value {
    fn from(value: &'s str) -> Self {
        let as_pg_str = &Str::from_str(value);
        Value::from_ptr(as_pg_str as *const _ as *mut u8)
    }
}
