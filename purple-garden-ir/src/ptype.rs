//! Purple garden type system
use std::{alloc::Layout, borrow::Cow, collections::HashMap, fmt::Display, hash::Hash};

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
    /// Option layout is:
    ///
    /// ```text
    /// byte  0        8        16
    ///       +--------+--------+
    ///       | tag    | ptr    |
    ///       +--------+--------+
    /// ```
    ///
    /// Where tag is either 0 for None or 1 for Some
    ///
    /// PERF: Niche optimisation to avoid heap allocs via `Type::option_repr -> OptionRepr { NullPtr
    /// | NaNSentinal | BoolSentinal | Tagged }`
    /// - optimise to use nullptr for None and ptr for some, thus just being 8 byte
    /// - optimise to use NaN values for Option(Double)
    /// - optimise to use non 0..1 values for Option(Bool)
    Option(BoxedType<'t>),
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
    Array(BoxedType<'t>),
    /// Records are stored inline:
    ///
    /// ```text
    /// Record<a: Int b: Record<c: Bool d: Str> e: Double>
    ///
    /// byte  0        8        16       24       32
    ///       +--------+--------+--------+--------+
    ///       | a      | b.c    | b.d    | e      |
    ///       +--------+--------+--------+--------+
    /// ```
    Record(RecordFields<'t>),
    /// Foreign type for handling opaque rust data feed into the vm runtime
    ///
    /// which is useful for something like Foreign<counter> vs
    /// Foreign<player> in the typesystem, meaning functions defined on the former can not be
    /// called on the latter, resulting in a type error
    Foreign(&'t str),
    /// Slot is part of the generic implementation for purple garden, at type checking time this is
    /// just an identifier denoting the slot to be replaced with a concrete type, for instance an
    /// optional over a generic is represented as Option(Slot("T"))
    Slot(&'t str),
}

/// Cow-style wrapper around the inner type of `Type::Option` / `Type::Array`.
///
/// `Static` lets these variants be constructed in `const` contexts (where
/// `Box::new` is not available), while `Owned` remains available for runtime
/// construction where the inner type is computed dynamically.
#[derive(Debug, Clone)]
pub enum BoxedType<'t> {
    Static(&'t Type<'t>),
    Owned(Box<Type<'t>>),
}

impl<'t> BoxedType<'t> {
    #[must_use]
    pub const fn static_type(ty: &'t Type<'t>) -> Self {
        Self::Static(ty)
    }

    #[must_use]
    pub fn owned(ty: Type<'t>) -> Self {
        Self::Owned(Box::new(ty))
    }

    #[must_use]
    pub fn as_ref(&self) -> &Type<'t> {
        match self {
            Self::Static(ty) => ty,
            Self::Owned(ty) => ty,
        }
    }
}

impl PartialEq for BoxedType<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl Eq for BoxedType<'_> {}

impl Hash for BoxedType<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

impl<'t> From<Box<Type<'t>>> for BoxedType<'t> {
    fn from(value: Box<Type<'t>>) -> Self {
        Self::Owned(value)
    }
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

/// Why [`Type::bind_slots`] refused to bind a slot to an argument type
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum BindError<'t> {
    /// The slot was already bound to a different type by an earlier argument
    Conflict {
        slot: &'t str,
        existing: Type<'t>,
        new: Type<'t>,
    },
    /// Generics can not be instantiated with `Void`
    Void { slot: &'t str },
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

    /// Recursively binds parameter slots to the matching argument type
    ///
    /// For example:
    ///
    /// ```text
    /// T           | Int                   => T := Int
    /// Option(T)   | Option(Int)           => T := Int
    /// Array(T)    | Array(Bool)           => T := Bool
    /// Option(T)   | Option(Option(Bool))  => T := Option(Bool)
    /// Record(t T) | Record(t Str)         => T := Str
    /// ```
    ///
    /// Shape mismatches (`Option(T) | Int`) bind nothing and are left to the caller to diagnose
    /// once slots are substituted.
    ///
    /// # Panics
    ///
    /// If `possible_filler` contains a slot: argument types are always concrete, a slot on that
    /// side means a generic return type escaped without substitution.
    pub fn bind_slots(
        &self,
        possible_filler: &Self,
        bindings: &mut HashMap<&'t str, Self>,
    ) -> Result<(), BindError<'t>> {
        match (self, possible_filler) {
            (_, Self::Slot(_)) => {
                unreachable!("argument type {possible_filler} contains an unsubstituted slot")
            }
            (Self::Option(lhs), Self::Option(rhs)) | (Self::Array(lhs), Self::Array(rhs)) => {
                lhs.as_ref().bind_slots(rhs.as_ref(), bindings)
            }
            (Self::Record(lhs), Self::Record(rhs)) => lhs
                .as_slice()
                .iter()
                .zip(rhs.as_slice())
                .try_for_each(|(l, r)| l.ty.bind_slots(&r.ty, bindings)),
            (Self::Slot(slot), Self::Void) => Err(BindError::Void { slot }),
            // T | a => T := a
            (Self::Slot(slot), a) => match bindings.get(slot) {
                None => {
                    bindings.insert(slot, a.clone());
                    Ok(())
                }
                Some(existing) if existing == a => Ok(()),
                Some(existing) => Err(BindError::Conflict {
                    slot,
                    existing: existing.clone(),
                    new: a.clone(),
                }),
            },
            _ => Ok(()),
        }
    }

    /// Recursively replaces every slot with its binding from `bindings`, the inverse of
    /// [`Type::bind_slots`]. Slots without a binding are left in place.
    ///
    /// Returns `Cow::Borrowed` when nothing was substituted so concrete types are never cloned.
    #[must_use]
    pub fn apply_slot_binding<'s>(&'s self, bindings: &HashMap<&'t str, Self>) -> Cow<'s, Self> {
        match self {
            Self::Slot(slot) => bindings
                .get(slot)
                .map_or(Cow::Borrowed(self), |ty| Cow::Owned(ty.clone())),
            Self::Option(inner) | Self::Array(inner) => {
                match inner.as_ref().apply_slot_binding(bindings) {
                    Cow::Borrowed(_) => Cow::Borrowed(self),
                    Cow::Owned(inner) => {
                        let inner = BoxedType::owned(inner);
                        Cow::Owned(match self {
                            Self::Option(_) => Self::Option(inner),
                            _ => Self::Array(inner),
                        })
                    }
                }
            }
            Self::Record(fields) => {
                let fields = fields.as_slice();
                let substituted: Vec<_> = fields
                    .iter()
                    .map(|field| field.ty.apply_slot_binding(bindings))
                    .collect();
                if substituted.iter().all(|ty| matches!(ty, Cow::Borrowed(_))) {
                    return Cow::Borrowed(self);
                }
                Cow::Owned(Self::Record(RecordFields::owned(
                    fields
                        .iter()
                        .zip(substituted)
                        .map(|(field, ty)| Field {
                            name: field.name,
                            ty: ty.into_owned(),
                        })
                        .collect(),
                )))
            }
            _ => Cow::Borrowed(self),
        }
    }

    /// Runtime payload layout for a value of this type, see [Type] for heap layouts
    pub fn layout(&self) -> Layout {
        Layout::from_size_align(self.size(), self.align()).expect("type layout")
    }

    pub fn size(&self) -> usize {
        match self {
            Type::Void => 0,
            Type::Slot(_) => {
                unreachable!("slots size asked, this is not supposed to happen")
            }
            Type::Record(fields) => record_size(fields.as_slice()),
            Type::Option(inner) => WORD_SIZE + inner.as_ref().size(),
            Type::Bool
            | Type::Int
            | Type::Double
            | Type::Str
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
            Type::Slot(slot_name) => write!(f, "{slot_name}"),
            Type::Void => write!(f, "Void"),
            Type::Bool => write!(f, "Bool"),
            Type::Int => write!(f, "Int"),
            Type::Double => write!(f, "Double"),
            Type::Str => write!(f, "Str"),
            Type::Foreign(id) => write!(f, "Foreign<{id}>"),
            Type::Option(inner) => write!(f, "Option<{}>", inner.as_ref()),
            Type::Array(inner) => write!(f, "Array<{}>", inner.as_ref()),
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
    use std::{borrow::Cow, collections::HashMap};

    use super::{BindError, BoxedType, Field, RecordFields, Type};

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
        let ty = Type::Array(BoxedType::owned(Type::Int));

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

    fn opt(ty: Type<'static>) -> Type<'static> {
        Type::Option(BoxedType::owned(ty))
    }

    fn arr(ty: Type<'static>) -> Type<'static> {
        Type::Array(BoxedType::owned(ty))
    }

    fn bind(param: &Type<'static>, arg: &Type<'static>) -> HashMap<&'static str, Type<'static>> {
        let mut bindings = HashMap::new();
        param.bind_slots(arg, &mut bindings).unwrap();
        bindings
    }

    #[test]
    fn bind_slots_binds_bare_slot() {
        let b = bind(&Type::Slot("T"), &Type::Int);
        assert_eq!(b, HashMap::from([("T", Type::Int)]));
    }

    #[test]
    fn bind_slots_looks_through_option_and_array() {
        assert_eq!(
            bind(&opt(Type::Slot("T")), &opt(Type::Int)),
            HashMap::from([("T", Type::Int)])
        );
        assert_eq!(
            bind(&arr(Type::Slot("T")), &arr(Type::Bool)),
            HashMap::from([("T", Type::Bool)])
        );
    }

    #[test]
    fn bind_slots_binds_nested_type() {
        assert_eq!(
            bind(&opt(Type::Slot("T")), &opt(opt(Type::Bool))),
            HashMap::from([("T", opt(Type::Bool))])
        );
        assert_eq!(
            bind(&arr(opt(Type::Slot("T"))), &arr(opt(Type::Str))),
            HashMap::from([("T", Type::Str)])
        );
    }

    #[test]
    fn bind_slots_binds_record_fields() {
        let param = Type::record(vec![("t", Type::Slot("T"))]);
        let arg = Type::record(vec![("t", Type::Str)]);
        assert_eq!(bind(&param, &arg), HashMap::from([("T", Type::Str)]));

        let param = Type::record(vec![
            ("a", Type::Slot("T")),
            ("b", Type::Int),
            ("c", opt(Type::Slot("U"))),
        ]);
        let arg = Type::record(vec![
            ("a", Type::Double),
            ("b", Type::Int),
            ("c", opt(Type::Bool)),
        ]);
        assert_eq!(
            bind(&param, &arg),
            HashMap::from([("T", Type::Double), ("U", Type::Bool)])
        );
    }

    #[test]
    fn bind_slots_binds_static_record_fields() {
        static PARAM: &[Field<'static>] = &[Field {
            name: "t",
            ty: Type::Slot("T"),
        }];
        let param = Type::Record(RecordFields::static_fields(PARAM));
        let arg = Type::record(vec![("t", Type::Int)]);
        assert_eq!(bind(&param, &arg), HashMap::from([("T", Type::Int)]));
    }

    #[test]
    fn bind_slots_ignores_concrete_types() {
        assert!(bind(&Type::Int, &Type::Int).is_empty());
        assert!(bind(&opt(Type::Int), &opt(Type::Str)).is_empty());
        assert!(
            bind(
                &Type::record(vec![("a", Type::Int)]),
                &Type::record(vec![("a", Type::Int)])
            )
            .is_empty()
        );
    }

    #[test]
    #[should_panic(expected = "argument type T contains an unsubstituted slot")]
    fn bind_slots_panics_on_argument_side_slot() {
        bind(&Type::Int, &Type::Slot("T"));
    }

    #[test]
    #[should_panic(expected = "argument type Option<T> contains an unsubstituted slot")]
    fn bind_slots_panics_on_nested_argument_side_slot() {
        bind(&Type::Slot("T"), &opt(Type::Slot("T")));
    }

    #[test]
    fn bind_slots_rejects_void() {
        let mut bindings = HashMap::new();
        assert_eq!(
            Type::Slot("T").bind_slots(&Type::Void, &mut bindings),
            Err(BindError::Void { slot: "T" })
        );
        assert_eq!(
            opt(Type::Slot("T")).bind_slots(&opt(Type::Void), &mut bindings),
            Err(BindError::Void { slot: "T" })
        );
        assert!(bindings.is_empty());
    }

    #[test]
    fn bind_slots_accepts_same_type_twice() {
        let param = Type::record(vec![("a", Type::Slot("T")), ("b", Type::Slot("T"))]);
        let arg = Type::record(vec![("a", Type::Int), ("b", Type::Int)]);
        assert_eq!(bind(&param, &arg), HashMap::from([("T", Type::Int)]));
    }

    #[test]
    fn bind_slots_rejects_conflicting_binding() {
        let param = Type::record(vec![("a", Type::Slot("T")), ("b", opt(Type::Slot("T")))]);
        let arg = Type::record(vec![("a", Type::Int), ("b", opt(Type::Str))]);
        let mut bindings = HashMap::new();
        assert_eq!(
            param.bind_slots(&arg, &mut bindings),
            Err(BindError::Conflict {
                slot: "T",
                existing: Type::Int,
                new: Type::Str,
            })
        );

        // conflicts across separate arguments
        let mut bindings = HashMap::new();
        Type::Slot("T")
            .bind_slots(&Type::Int, &mut bindings)
            .unwrap();
        assert_eq!(
            arr(Type::Slot("T")).bind_slots(&arr(Type::Bool), &mut bindings),
            Err(BindError::Conflict {
                slot: "T",
                existing: Type::Int,
                new: Type::Bool,
            })
        );
        assert_eq!(bindings, HashMap::from([("T", Type::Int)]));
    }

    #[test]
    fn bind_slots_ignores_shape_mismatch() {
        assert!(bind(&opt(Type::Slot("T")), &arr(Type::Int)).is_empty());
        assert!(bind(&opt(Type::Slot("T")), &Type::Int).is_empty());
        assert!(bind(&arr(Type::Slot("T")), &Type::Void).is_empty());
    }

    #[test]
    fn bind_slots_accumulates_across_calls() {
        let mut bindings = HashMap::new();
        Type::Slot("T")
            .bind_slots(&Type::Int, &mut bindings)
            .unwrap();
        opt(Type::Slot("U"))
            .bind_slots(&opt(Type::Str), &mut bindings)
            .unwrap();
        assert_eq!(
            bindings,
            HashMap::from([("T", Type::Int), ("U", Type::Str)])
        );
    }

    #[test]
    fn apply_slot_binding_substitutes_bare_and_nested_slots() {
        let b = HashMap::from([("T", Type::Int), ("U", opt(Type::Str))]);
        assert_eq!(*Type::Slot("T").apply_slot_binding(&b), Type::Int);
        assert_eq!(*opt(Type::Slot("T")).apply_slot_binding(&b), opt(Type::Int));
        assert_eq!(
            *arr(opt(Type::Slot("U"))).apply_slot_binding(&b),
            arr(opt(opt(Type::Str)))
        );
        assert_eq!(
            *Type::record(vec![("a", Type::Slot("T")), ("b", Type::Bool), ("c", Type::Slot("U"))])
                .apply_slot_binding(&b),
            Type::record(vec![("a", Type::Int), ("b", Type::Bool), ("c", opt(Type::Str))])
        );
    }

    #[test]
    fn apply_slot_binding_substitutes_static_record_fields() {
        static FIELDS: &[Field<'static>] = &[Field {
            name: "t",
            ty: Type::Slot("T"),
        }];
        let b = HashMap::from([("T", Type::Int)]);
        assert_eq!(
            *Type::Record(RecordFields::static_fields(FIELDS)).apply_slot_binding(&b),
            Type::record(vec![("t", Type::Int)])
        );
    }

    #[test]
    fn apply_slot_binding_leaves_unbound_slots_and_borrows_when_unchanged() {
        let b = HashMap::from([("T", Type::Int)]);
        for ty in [
            Type::Int,
            opt(Type::Str),
            Type::Slot("U"),
            arr(Type::Slot("U")),
            Type::record(vec![("a", Type::Bool), ("u", Type::Slot("U"))]),
        ] {
            let out = ty.apply_slot_binding(&b);
            assert!(matches!(out, Cow::Borrowed(_)), "{ty} should not be cloned");
            assert_eq!(*out, ty);
        }
        assert!(matches!(
            Type::Slot("T").apply_slot_binding(&HashMap::new()),
            Cow::Borrowed(Type::Slot("T"))
        ));
    }
}
