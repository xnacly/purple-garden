use std::{
    fmt::{self, Debug, Display},
    hint::unreachable_unchecked,
};

use crate::ir::Const;

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

/// NaN boxed purple garden virtual machine value representation
///
/// ```text
/// | Bits  | Usage                               |
/// | ----- | ----------------------------------- |
/// | 63:52 | Exponent of double / NaN tag        |
/// | 51:48 | Type tag for NaN-boxed values       |
/// |  47:0 | Payload (int, pointer, small flags) |
/// ```
///
/// Specifically the bit tag (upper 16B):
///
/// ```text
/// | Type       | Tag (upper 16 bits) | Payload (lower 48 bits) |
/// | ---------- | ------------------- | ----------------------- |
/// | Int        | 0x7FF8              | signed 48-bit int       |
/// | Bool True  | 0x7FF9              | 1                       |
/// | Bool False | 0x7FF9              | 0                       |
/// | UnDef      | 0x7FFA              | 0                       |
/// | Str(&'v)   | 0x7FFB              | &str                    |
/// | String     | 0x7FFC              | String                  |
/// | Array      | 0x7FFD              | GC object               |
/// | Object     | 0x7FFE              | GC object               |
/// | Double     | normal f64 bits     | full 64 bits            |
/// ```
#[derive(PartialEq, Clone, Default, Copy)]
pub struct Value(u64);

impl Value {
    pub const INT: u64 = 0x7FF8 << 48;
    pub const BOOL: u64 = 0x7FF9 << 48;
    pub const UNDEF: u64 = 0x7FFA << 48;
    pub const STR: u64 = 0x7FFB << 48;
    pub const HEAPSTRING: u64 = 0x7FFC << 48;
    pub const ARRAY: u64 = 0x7FFD << 48;
    pub const OBJECT: u64 = 0x7FFE << 48;

    #[inline(always)]
    pub fn tag(&self) -> u64 {
        self.0 & 0xFFFF_0000_0000_0000
    }

    #[inline(always)]
    pub const fn undef() -> Self {
        Self(Self::UNDEF)
    }

    #[inline(always)]
    pub unsafe fn as_int(&self) -> i64 {
        debug_assert!(self.tag() == Self::INT);
        // sign extend 48-bit payload
        let payload = self.0 & 0x0000_FFFF_FFFF_FFFF;
        ((payload as i64) << 16) >> 16
    }

    #[inline(always)]
    /// True=1, False=0 in TAG
    pub unsafe fn as_bool(&self) -> bool {
        self.0 & 1 != 0
    }

    #[inline(always)]
    pub unsafe fn as_f64(&self) -> f64 {
        debug_assert!(self.tag() <= 0x7FF7 << 48);
        f64::from_bits(self.0)
    }

    pub unsafe fn as_str<'t>(&self) -> &'t str {
        unsafe {
            debug_assert!(self.tag() == Self::STR || self.tag() == Self::HEAPSTRING);
            let wrapper = self.as_ptr::<Str>();
            (*wrapper).as_str()
        }
    }

    #[inline(always)]
    pub unsafe fn as_ptr<T>(&self) -> *mut T {
        debug_assert!(matches!(
            self.tag(),
            Self::STR | Self::HEAPSTRING | Self::ARRAY | Self::OBJECT
        ));
        (self.0 & 0x0000_FFFF_FFFF_FFFF) as *mut T
    }

    #[inline(always)]
    pub fn from_ptr<T>(ptr: *mut T, tag: u64) -> Self {
        Self(tag | ((ptr as u64) & 0x0000_FFFF_FFFF_FFFF))
    }

    pub fn compare(&self, other: &Self) -> bool {
        let (lt, rt) = (self.tag(), other.tag());

        unsafe {
            if lt == rt {
                match lt {
                    Self::INT => self.as_int() == other.as_int(),
                    Self::BOOL => self.as_bool() == other.as_bool(),
                    Self::STR | Self::HEAPSTRING => self.as_str() == other.as_str(),
                    _ => unreachable_unchecked(),
                }
            } else {
                // If tags differ, it could be f64
                if lt == 0 && rt == 0 {
                    self.as_f64() == other.as_f64()
                } else {
                    false
                }
            }
        }
    }
}

impl<'c> From<Const<'c>> for Value {
    fn from(value: Const<'c>) -> Self {
        Self(match value {
            Const::False => Self::BOOL,
            Const::True => Self::BOOL | 1,
            Const::Int(i) => Self::INT | ((i as u64) & 0x0000_FFFF_FFFF_FFFF),
            Const::Double(bits) => bits,
            Const::Str(str) => {
                let as_pg_str = &Str::from_str(str);
                return Value::from_ptr(as_pg_str as *const _ as *mut u8, Self::STR);
            }
        })
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self(Self::BOOL | if value { 1 } else { 0 })
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self(Self::INT | ((value as u64) & 0x0000_FFFF_FFFF_FFFF))
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
        Value::from_ptr(as_pg_str as *const _ as *mut u8, Self::STR)
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unsafe {
            match self.tag() {
                Value::INT => write!(f, "{}", self.as_int()),
                Value::BOOL => write!(f, "{}", self.as_bool()),
                Value::UNDEF => write!(f, "undefined"),
                Value::STR | Value::HEAPSTRING => write!(f, "\"{}\"", self.as_str()),
                // Value::ARRAY => write!(f, "{}", self.as_()),
                // Value::OBJECT => write!(f, "{}", self.as_()),
                _ => write!(f, "{}", self.as_f64()),
            }
        }
    }
}

impl Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut start = f.debug_struct("Value");
        start.field(
            "type",
            &match self.tag() {
                Value::INT => "int",
                Value::BOOL => "bool",
                Value::UNDEF => "undef",
                Value::STR => "str",
                Value::HEAPSTRING => "heapstring",
                Value::ARRAY => "arr",
                Value::OBJECT => "obj",
                _ => "double",
            },
        );
        start.field("raw", &format!("0x{:0x}", self.tag()));
        unsafe {
            match self.tag() {
                Value::INT => start.field("val", &self.as_int()).finish(),
                Value::BOOL => start.field("val", &self.as_bool()).finish(),
                Value::UNDEF => start.field("val", &"undefined").finish(),
                Value::STR | Value::HEAPSTRING => start.field("val", &self.as_str()).finish(),
                _ => start.field("val", &self.as_f64()).finish(),
            }
        }
    }
}
