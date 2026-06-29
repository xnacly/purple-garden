//! Purple garden type system
use std::{alloc::Layout, fmt::Display};

use crate::Const;

const WORD_SIZE: usize = 8;
const WORD_ALIGN: usize = 8;

/// Compile time type system,
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum Type<'t> {
    Void,
    Bool,
    Int,
    Double,
    Str,
    Option(Box<Type<'t>>),
    Array(Box<Type<'t>>),
    Record(Vec<(&'t str, Type<'t>)>),
    // Foreign type for handling opaque rust data feed into the vm runtime
    //
    // which is useful for something like Foreign<counter> vs
    // Foreign<player> in the typesystem, meaning functions defined on the former can not be
    // called on the latter, resulting in a type error
    Foreign(&'t str),
}

impl<'t> Type<'t> {
    /// Runtime payload layout for values of this type.
    ///
    /// Records are inline: nested record fields contribute their full payload
    /// size, not one pointer-sized slot. Recursive layout does not need cycle
    /// detection because record types are anonymous structural values.
    pub fn layout(&self) -> Layout {
        Layout::from_size_align(self.size(), self.align()).expect("type layout")
    }

    pub fn size(&self) -> usize {
        match self {
            Type::Void => 0,
            Type::Record(fields) => fields
                .last()
                .map_or(0, |(name, ty)| self.field_offset(name).unwrap() + ty.size()),
            Type::Bool
            | Type::Int
            | Type::Double
            | Type::Str
            | Type::Option(_)
            | Type::Array(_)
            | Type::Foreign(_) => WORD_SIZE,
        }
    }

    pub fn align(&self) -> usize {
        match self {
            Type::Void => 1,
            Type::Record(fields) => fields
                .iter()
                .map(|(_, ty)| ty.align())
                .max()
                .unwrap_or(WORD_ALIGN),
            _ => WORD_ALIGN,
        }
    }

    pub fn field_offset(&self, name: &str) -> Option<usize> {
        let Type::Record(fields) = self else {
            return None;
        };

        let mut offset = 0;
        for (field, ty) in fields {
            offset = align_up(offset, ty.align());
            if *field == name {
                return Some(offset);
            }
            offset += ty.size();
        }

        None
    }
}

fn align_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (value + align - 1) & !(align - 1)
}

impl Display for Type<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Void => write!(f, "Void"),
            Type::Bool => write!(f, "Bool"),
            Type::Int => write!(f, "Int"),
            Type::Double => write!(f, "Double"),
            Type::Str => write!(f, "Str"),
            Type::Foreign(id) => write!(f, "Foreign<{id}>"),
            Type::Option(inner) => write!(f, "Option<{inner}>"),
            Type::Array(inner) => write!(f, "Array<{inner}>"),
            Type::Record(fields) => {
                write!(f, "Record<")?;
                for (i, (key, value)) in fields.iter().enumerate() {
                    write!(f, "{key}: {value}")?;
                    if i + 1 != fields.len() {
                        write!(f, " ")?;
                    }
                }
                write!(f, ">")
            }
        }
    }
}

impl<'a> From<Const<'a>> for Type<'a> {
    fn from(value: Const<'_>) -> Self {
        match value {
            Const::True | Const::False => Self::Bool,
            Const::Int(_) => Self::Int,
            Const::Double(_) => Self::Double,
            Const::Str(_) => Self::Str,
            Const::Undefined => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Type;

    #[test]
    fn scalars_are_one_vm_word() {
        for ty in [Type::Bool, Type::Int, Type::Double, Type::Str] {
            assert_eq!(ty.size(), 8);
            assert_eq!(ty.align(), 8);
        }
    }

    #[test]
    fn records_are_inline() {
        // Record<a: Bool nested: Record<b: Str c: Int> d: Double>
        //
        // byte  0        8        16       24       32
        //       +--------+--------+--------+--------+
        //       | a      | nested | nested | d      |
        //       |        | .b     | .c     |        |
        //       +--------+--------+--------+--------+
        let ty = Type::Record(vec![
            ("a", Type::Bool),
            (
                "nested",
                Type::Record(vec![("b", Type::Str), ("c", Type::Int)]),
            ),
            ("d", Type::Double),
        ]);

        assert_eq!(ty.size(), 32);
        assert_eq!(ty.align(), 8);
        assert_eq!(ty.field_offset("a"), Some(0));
        assert_eq!(ty.field_offset("nested"), Some(8));
        assert_eq!(ty.field_offset("d"), Some(24));
        assert_eq!(ty.field_offset("missing"), None);
    }
}
