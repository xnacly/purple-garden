//! Purple garden type system
use std::{
    alloc::{Allocator, Layout},
    collections::HashMap,
    fmt::Display,
    hash::BuildHasher,
};

use purple_garden_allocators::bump::Arena;

use crate::Const;

// TODO: replace with mem::{size_of,align_of}
const WORD_SIZE: usize = 8;
const WORD_ALIGN: usize = 8;

/// Compile time type system, compound types point into an [`Arena`] or are `'static`
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
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
    Option(&'t Type<'t>),
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
    Array(&'t Type<'t>),
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
    Record(&'t [Field<'t>]),
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

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
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

impl<'t> Type<'t> {
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
    pub fn bind_slots<S: BuildHasher, A: Allocator>(
        &self,
        possible_filler: &Self,
        bindings: &mut HashMap<&'t str, Self, S, A>,
    ) -> Result<(), BindError<'t>> {
        match (self, possible_filler) {
            (_, Self::Slot(_)) => {
                unreachable!("argument type {possible_filler} contains an unsubstituted slot")
            }
            (Self::Option(lhs), Self::Option(rhs)) | (Self::Array(lhs), Self::Array(rhs)) => {
                lhs.bind_slots(rhs, bindings)
            }
            (Self::Record(lhs), Self::Record(rhs)) => lhs
                .iter()
                .zip(rhs.iter())
                .try_for_each(|(l, r)| l.ty.bind_slots(&r.ty, bindings)),
            (Self::Slot(slot), Self::Void) => Err(BindError::Void { slot }),
            // T | a => T := a
            (Self::Slot(slot), a) => match bindings.get(slot) {
                None => {
                    debug_assert!(
                        !a.contains_slot(),
                        "argument type {a} contains an unsubstituted slot"
                    );
                    bindings.insert(slot, *a);
                    Ok(())
                }
                Some(existing) if existing == a => Ok(()),
                Some(existing) => Err(BindError::Conflict {
                    slot,
                    existing: *existing,
                    new: *a,
                }),
            },
            _ => Ok(()),
        }
    }

    fn has_bound_slot<S: BuildHasher, A: Allocator>(
        &self,
        bindings: &HashMap<&'t str, Self, S, A>,
    ) -> bool {
        match self {
            Self::Slot(slot) => bindings.contains_key(slot),
            Self::Option(inner) | Self::Array(inner) => inner.has_bound_slot(bindings),
            Self::Record(fields) => fields.iter().any(|f| f.ty.has_bound_slot(bindings)),
            _ => false,
        }
    }

    fn contains_slot(&self) -> bool {
        match self {
            Self::Slot(_) => true,
            Self::Option(inner) | Self::Array(inner) => inner.contains_slot(),
            Self::Record(fields) => fields.iter().any(|f| f.ty.contains_slot()),
            _ => false,
        }
    }

    /// Recursively replaces every slot with its binding from `bindings`, the inverse of
    /// [`Type::bind_slots`]. Slots without a binding are left in place, types without slots are
    /// returned as is, so only substituted compound types allocate in `arena`.
    #[must_use]
    pub fn apply_slot_binding<S: BuildHasher, A: Allocator>(
        &self,
        bindings: &HashMap<&'t str, Self, S, A>,
        arena: &'t impl Arena,
    ) -> Self {
        if !self.has_bound_slot(bindings) {
            return *self;
        }
        match self {
            Self::Slot(slot) => bindings.get(slot).copied().unwrap_or(*self),
            Self::Option(inner) => {
                Self::Option(arena.alloc(inner.apply_slot_binding(bindings, arena)))
            }
            Self::Array(inner) => {
                Self::Array(arena.alloc(inner.apply_slot_binding(bindings, arena)))
            }
            Self::Record(fields) => {
                Self::Record(arena.alloc_slice(fields.iter().map(|field| Field {
                    name: field.name,
                    ty: field.ty.apply_slot_binding(bindings, arena),
                })))
            }
            _ => *self,
        }
    }

    /// Copies `self` and every type it points to into `arena`.
    #[must_use]
    pub fn copy_in<'b>(&self, arena: &'b impl Arena) -> Type<'b>
    where
        't: 'b,
    {
        match *self {
            Self::Option(inner) => Type::Option(arena.alloc(inner.copy_in(arena))),
            Self::Array(inner) => Type::Array(arena.alloc(inner.copy_in(arena))),
            Self::Record(fields) => {
                Type::Record(arena.alloc_slice(fields.iter().map(|field| Field {
                    name: field.name,
                    ty: field.ty.copy_in(arena),
                })))
            }
            other => other,
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
            Type::Record(fields) => record_size(fields),
            Type::Option(inner) => WORD_SIZE + inner.size(),
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

        field_offset(fields, name)
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
            Type::Option(inner) => write!(f, "Option<{inner}>"),
            Type::Array(inner) => write!(f, "Array<{inner}>"),
            Type::Record(fields) => {
                write!(f, "Record<")?;
                for (i, field) in fields.iter().enumerate() {
                    write!(f, "{}: {}", field.name, field.ty)?;
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
    use std::collections::HashMap;

    use purple_garden_allocators::{bump::BumpAlloc, metric::MetricAlloc};

    use super::{BindError, Field, Type};

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
        let ty = record(vec![
            ("a", Type::Bool),
            ("nested", record(vec![("b", Type::Str), ("c", Type::Int)])),
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
        let ty = Type::Array(&Type::Int);

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

        let static_record = Type::Record(FIELDS);
        let owned_record = Type::Record(FIELDS.to_vec().leak());

        assert_eq!(static_record, owned_record);
    }

    fn opt(ty: Type<'static>) -> Type<'static> {
        Type::Option(Box::leak(Box::new(ty)))
    }

    fn arr(ty: Type<'static>) -> Type<'static> {
        Type::Array(Box::leak(Box::new(ty)))
    }

    fn record(fields: Vec<(&'static str, Type<'static>)>) -> Type<'static> {
        Type::Record(
            fields
                .into_iter()
                .map(|(name, ty)| Field { name, ty })
                .collect::<Vec<_>>()
                .leak(),
        )
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
        let param = record(vec![("t", Type::Slot("T"))]);
        let arg = record(vec![("t", Type::Str)]);
        assert_eq!(bind(&param, &arg), HashMap::from([("T", Type::Str)]));

        let param = record(vec![
            ("a", Type::Slot("T")),
            ("b", Type::Int),
            ("c", opt(Type::Slot("U"))),
        ]);
        let arg = record(vec![
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
        let param = Type::Record(PARAM);
        let arg = record(vec![("t", Type::Int)]);
        assert_eq!(bind(&param, &arg), HashMap::from([("T", Type::Int)]));
    }

    #[test]
    fn bind_slots_ignores_concrete_types() {
        assert!(bind(&Type::Int, &Type::Int).is_empty());
        assert!(bind(&opt(Type::Int), &opt(Type::Str)).is_empty());
        assert!(
            bind(
                &record(vec![("a", Type::Int)]),
                &record(vec![("a", Type::Int)])
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
        let param = record(vec![("a", Type::Slot("T")), ("b", Type::Slot("T"))]);
        let arg = record(vec![("a", Type::Int), ("b", Type::Int)]);
        assert_eq!(bind(&param, &arg), HashMap::from([("T", Type::Int)]));
    }

    #[test]
    fn bind_slots_rejects_conflicting_binding() {
        let param = record(vec![("a", Type::Slot("T")), ("b", opt(Type::Slot("T")))]);
        let arg = record(vec![("a", Type::Int), ("b", opt(Type::Str))]);
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
        let arena = BumpAlloc::new();
        let b = HashMap::from([("T", Type::Int), ("U", opt(Type::Str))]);
        assert_eq!(Type::Slot("T").apply_slot_binding(&b, &arena), Type::Int);
        assert_eq!(
            opt(Type::Slot("T")).apply_slot_binding(&b, &arena),
            opt(Type::Int)
        );
        assert_eq!(
            arr(opt(Type::Slot("U"))).apply_slot_binding(&b, &arena),
            arr(opt(opt(Type::Str)))
        );
        assert_eq!(
            record(vec![
                ("a", Type::Slot("T")),
                ("b", Type::Bool),
                ("c", Type::Slot("U"))
            ])
            .apply_slot_binding(&b, &arena),
            record(vec![
                ("a", Type::Int),
                ("b", Type::Bool),
                ("c", opt(Type::Str))
            ])
        );
    }

    #[test]
    fn apply_slot_binding_substitutes_static_record_fields() {
        static FIELDS: &[Field<'static>] = &[Field {
            name: "t",
            ty: Type::Slot("T"),
        }];
        let arena = BumpAlloc::new();
        let b = HashMap::from([("T", Type::Int)]);
        assert_eq!(
            Type::Record(FIELDS).apply_slot_binding(&b, &arena),
            record(vec![("t", Type::Int)])
        );
    }

    #[test]
    fn apply_slot_binding_leaves_unbound_slots_and_allocates_only_substitutions() {
        let arena = MetricAlloc::new(BumpAlloc::new());
        let b = HashMap::from([("T", Type::Int)]);
        for ty in [
            Type::Int,
            opt(Type::Str),
            Type::Slot("U"),
            arr(Type::Slot("U")),
            record(vec![("a", Type::Bool), ("u", Type::Slot("U"))]),
        ] {
            assert_eq!(ty.apply_slot_binding(&b, &arena), ty);
        }
        assert_eq!(
            Type::Slot("T").apply_slot_binding(&HashMap::new(), &arena),
            Type::Slot("T")
        );
        assert_eq!(arena.metrics().allocs, 0);
    }
}
