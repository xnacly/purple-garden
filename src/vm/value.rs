use std::fmt::Debug;

use crate::{
    ir::{Const, ptype},
    vm,
};

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
    pub fn as_str<'t>(&self, pool: &'t [Box<str>]) -> &'t str {
        pool[self.0 as usize].as_ref()
    }

    #[inline(always)]
    pub fn as_ptr<T>(&self) -> *mut T {
        self.0 as *mut T
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
}

impl<'c> From<Const<'c>> for Value {
    fn from(value: Const<'c>) -> Self {
        Self(match value {
            Const::False => 0u64,
            Const::True => 1u64,
            Const::Int(i) => i as u64,
            Const::Double(bits) => bits,
            // my favorite placeholder
            _ => return Value(0xDEADAFFE),
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
