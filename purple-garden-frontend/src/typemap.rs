//! Node value id -> type storage shared by the typechecker and lowering
use std::alloc::{Allocator, Global};

use purple_garden_ir::ptype::Type;

/// Handle into the [`TypeMap`] arena, several slots may point at the same one
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeRef(u32);

/// Types keyed by node value id. Each `Type` lives once in an arena and slots point at it, so a
/// binding, its reads and the expression that produced it share one allocation
#[derive(Debug, Clone)]
pub struct TypeMap<'t, A: Allocator = Global> {
    slots: Vec<Option<TypeRef>, A>,
    arena: Vec<Type<'t>, A>,
}

impl Default for TypeMap<'_> {
    fn default() -> Self {
        Self::with_slots_in(0, Global)
    }
}

impl<'t, A: Allocator + Clone> TypeMap<'t, A> {
    /// Nearly every slot stores one fresh type, so the arena is sized to the slots up front
    #[must_use]
    pub fn with_slots_in(slots: usize, alloc: A) -> Self {
        let mut slot_vec = Vec::with_capacity_in(slots, alloc.clone());
        slot_vec.resize(slots, None);
        Self {
            slots: slot_vec,
            arena: Vec::with_capacity_in(slots, alloc),
        }
    }
}

impl<'t, A: Allocator> TypeMap<'t, A> {
    #[must_use]
    pub fn slots(&self) -> usize {
        self.slots.len()
    }

    #[must_use]
    #[inline]
    pub fn get(&self, slot: usize) -> Option<&Type<'t>> {
        self.ref_of(slot).map(|r| self.resolve(r))
    }

    #[must_use]
    #[inline]
    pub fn ref_of(&self, slot: usize) -> Option<TypeRef> {
        self.slots.get(slot).copied().flatten()
    }

    #[must_use]
    #[inline]
    pub fn resolve(&self, r: TypeRef) -> &Type<'t> {
        &self.arena[r.0 as usize]
    }

    #[inline]
    pub fn alloc(&mut self, ty: Type<'t>) -> TypeRef {
        let r = TypeRef(u32::try_from(self.arena.len()).expect("more than u32::MAX types"));
        self.arena.push(ty);
        r
    }

    /// Points `slot` at an existing type
    #[inline]
    pub fn bind(&mut self, slot: usize, r: TypeRef) {
        self.slots[slot] = Some(r);
    }

    #[inline]
    pub fn insert(&mut self, slot: usize, ty: Type<'t>) -> TypeRef {
        let r = self.alloc(ty);
        self.bind(slot, r);
        r
    }
}
