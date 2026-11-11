use std::{
    alloc::{AllocError, Allocator, Layout},
    ptr::NonNull,
};

use crate::{IntoVm, Value, gc::AllocType, string};

use super::Vm;

/// The allocator a [`Vm`] was built with. Builtins and native code receive the
/// VM as a pointer and cast it to one `Vm` type, so its allocator can't be a
/// type parameter and is erased here instead
pub struct VmAlloc<'vm>(pub(super) Box<dyn Allocator + 'vm>);

impl std::fmt::Debug for VmAlloc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VmAlloc")
    }
}

unsafe impl Allocator for VmAlloc<'_> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.0.allocate(layout)
    }

    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.0.allocate_zeroed(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        // SAFETY: caller upholds the Allocator contract
        unsafe { self.0.deallocate(ptr, layout) }
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        // SAFETY: caller upholds the Allocator contract
        unsafe { self.0.grow(ptr, old, new) }
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        // SAFETY: caller upholds the Allocator contract
        unsafe { self.0.grow_zeroed(ptr, old, new) }
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        // SAFETY: caller upholds the Allocator contract
        unsafe { self.0.shrink(ptr, old, new) }
    }
}

impl Vm<'_> {
    pub(super) fn collect(&mut self) {
        self.gc.collect();
    }

    pub(super) fn collect_noop(&mut self) {}

    pub fn try_alloc(
        &mut self,
        alloc_type: AllocType,
        layout: Layout,
    ) -> Option<std::ptr::NonNull<u8>> {
        if let Some(ptr) = self.gc.alloc_fast(alloc_type, layout) {
            return Some(ptr);
        }

        self.alloc_slow(alloc_type, layout)
    }

    #[cold]
    fn alloc_slow(
        &mut self,
        alloc_type: AllocType,
        layout: Layout,
    ) -> Option<std::ptr::NonNull<u8>> {
        (self.collect_fn)(self);

        if let Some(ptr) = self.gc.alloc_fast(alloc_type, layout) {
            return Some(ptr);
        }

        self.gc.grow(layout).ok()?;
        self.gc.alloc_fast(alloc_type, layout)
    }

    pub fn alloc(&mut self, alloc_type: AllocType, layout: Layout) -> std::ptr::NonNull<u8> {
        self.try_alloc(alloc_type, layout)
            .expect("GC allocation failed")
    }

    /// Boxes `value` as an Option, see [`crate::Type::Option`]. Every None is [`Vm::none`].
    pub fn new_option<T: IntoVm>(&mut self, value: Option<T>) -> Value {
        let Some(value) = value else {
            return self.none;
        };
        let payload = value.into_vm(self);
        self.option_box(1, payload)
    }

    pub(super) fn option_box(&mut self, tag: u64, payload: Value) -> Value {
        let block = self
            .alloc(AllocType::Option, Layout::new::<[u64; 2]>())
            .cast::<[u64; 2]>();
        // SAFETY: fresh allocation sized and aligned for [u64; 2]
        unsafe { block.write([tag, payload.0]) };
        Value(block.as_ptr() as u64)
    }

    pub fn new_string(&mut self, s: String) -> Value {
        self.new_string_from_str(&s)
    }

    /// Strings are interned: equal contents return the same payload.
    pub fn new_string_from_str(&mut self, s: &str) -> Value {
        let hash = string::hash(s.as_bytes());
        if let Some(v) = self.strings.get(s, hash) {
            return v;
        }
        let layout = Layout::from_size_align(string::payload_size(s.len()), string::ALIGN)
            .expect("string allocation layout");
        let payload = self.alloc(AllocType::String, layout).as_ptr();
        // SAFETY: payload allocated as payload_size(s.len()) at ALIGN
        let v = unsafe { string::write_payload(payload, s, hash) };
        // SAFETY: fresh string, get missed it
        unsafe { self.strings.insert(v) };
        v
    }

    /// [`Vm::new_string_from_str`] for the at most 8 bytes [`string::small_word`] packed into
    /// `word`, hashed and compared without them ever being in memory.
    ///
    /// # Safety
    ///
    /// The low `len` bytes of `word` are utf-8.
    pub unsafe fn new_string_from_small(&mut self, len: usize, word: u64) -> Value {
        let hash = string::small_hash(len, word);
        if let Some(v) = self.strings.get_small(len, word, hash) {
            return v;
        }
        let bytes = word.to_le_bytes();
        // SAFETY: caller upholds # Safety
        let s = unsafe { std::str::from_utf8_unchecked(&bytes[..len]) };
        let layout = Layout::from_size_align(string::payload_size(len), string::ALIGN)
            .expect("string allocation layout");
        let payload = self.alloc(AllocType::String, layout).as_ptr();
        // SAFETY: payload allocated as payload_size(len) at ALIGN
        let v = unsafe { string::write_payload(payload, s, hash) };
        // SAFETY: fresh string, get_small missed it
        unsafe { self.strings.insert(v) };
        v
    }
}
