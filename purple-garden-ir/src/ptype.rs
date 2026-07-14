//! Purple garden type system
use std::{alloc::Layout, fmt::Display, hash::Hash};

use crate::Const;

// TODO: replace with mem::{size_of,align_of}
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
    Record(RecordFields<'t>),
    // Foreign type for handling opaque rust data feed into the vm runtime
    //
    // which is useful for something like Foreign<counter> vs
    // Foreign<player> in the typesystem, meaning functions defined on the former can not be
    // called on the latter, resulting in a type error
    Foreign(&'t str),
}

#[derive(Debug, Clone)]
pub enum RecordFields<'t> {
    Static(&'t [Field<'t>]),
    Owned(Vec<Field<'t>>),
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct Field<'t> {
    pub name: &'t str,
    pub ty: Type<'t>,
}

impl<'t> RecordFields<'t> {
    #[must_use]
    pub const fn static_fields(fields: &'t [Field<'t>]) -> Self {
        Self::Static(fields)
    }

    #[must_use]
    pub fn owned(fields: Vec<Field<'t>>) -> Self {
        Self::Owned(fields)
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Field<'t>] {
        match self {
            Self::Static(fields) => fields,
            Self::Owned(fields) => fields,
        }
    }
}

impl PartialEq for RecordFields<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for RecordFields<'_> {}

impl Hash for RecordFields<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl<'t> From<Vec<Field<'t>>> for RecordFields<'t> {
    fn from(value: Vec<Field<'t>>) -> Self {
        Self::Owned(value)
    }
}

impl<'t> Type<'t> {
    #[must_use]
    pub fn record(fields: Vec<(&'t str, Type<'t>)>) -> Self {
        Self::Record(RecordFields::owned(
            fields
                .into_iter()
                .map(|(name, ty)| Field { name, ty })
                .collect(),
        ))
    }

    /// Runtime payload layout for a value of this type.
    ///
    /// Records are inline: nested record fields contribute their full payload
    /// size, not one pointer-sized slot. For example:
    ///
    /// ```text
    /// Record<a: Int b: Record<c: Bool d: Str> e: Double>
    ///
    /// byte  0        8        16       24       32
    ///       +--------+--------+--------+--------+
    ///       | a      | b.c    | b.d    | e      |
    ///       +--------+--------+--------+--------+
    /// ```
    ///
    /// Arrays are pointer-sized values. The heap payload behind an `Array<T>`
    /// is contiguous and starts with a length word:
    ///
    /// ```text
    /// Array<Record<x: Int y: Bool>> with len = 2
    ///
    /// byte  0        8        16       24       32       40
    ///       +--------+--------+--------+--------+--------+
    ///       | len    | [0].x  | [0].y  | [1].x  | [1].y  |
    ///       +--------+--------+--------+--------+--------+
    /// ```
    pub fn layout(&self) -> Layout {
        Layout::from_size_align(self.size(), self.align()).expect("type layout")
    }

    pub fn size(&self) -> usize {
        match self {
            Type::Void => 0,
            Type::Record(fields) => record_size(fields.as_slice()),
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
                .as_slice()
                .iter()
                .map(|field| field.ty.align())
                .max()
                .unwrap_or(WORD_ALIGN),
            _ => WORD_ALIGN,
        }
    }

    pub fn field_offset(&self, name: &str) -> Option<usize> {
        let Type::Record(fields) = self else {
            return None;
        };

        field_offset(fields.as_slice(), name)
    }
}

fn record_size(fields: &[Field<'_>]) -> usize {
    fields.iter().fold(0, |offset, field| {
        align_up(offset, field.ty.align()) + field.ty.size()
    })
}

fn field_offset(fields: &[Field<'_>], name: &str) -> Option<usize> {
    let mut offset = 0;
    for field in fields {
        offset = align_up(offset, field.ty.align());
        if field.name == name {
            return Some(offset);
        }
        offset += field.ty.size();
    }

    None
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
                for (i, field) in fields.as_slice().iter().enumerate() {
                    write!(f, "{}: {}", field.name, field.ty)?;
                    if i + 1 != fields.as_slice().len() {
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
    use super::{Field, RecordFields, Type};

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
        let ty = Type::record(vec![
            ("a", Type::Bool),
            (
                "nested",
                Type::record(vec![("b", Type::Str), ("c", Type::Int)]),
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

    #[test]
    fn array_values_are_pointers_to_payloads() {
        let ty = Type::Array(Box::new(Type::Int));

        assert_eq!(ty.size(), 8);
        assert_eq!(ty.align(), 8);
    }

    #[test]
    fn record_fields_compare_structurally() {
        static FIELDS: &[Field<'static>] = &[
            Field {
                name: "name",
                ty: Type::Str,
            },
            Field {
                name: "age",
                ty: Type::Int,
            },
        ];

        let static_record = Type::Record(RecordFields::static_fields(FIELDS));
        let owned_record = Type::Record(RecordFields::owned(FIELDS.to_vec()));

        assert_eq!(static_record, owned_record);
    }
}
