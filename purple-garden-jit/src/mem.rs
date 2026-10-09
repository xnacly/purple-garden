//! Executable memory for JIT'd code, its pages come from a [`Pages`] allocator.

use purple_garden_allocators::page::{
    PageAlloc, Pages,
    mmap::{self, MmapProt},
};
use purple_garden_runtime::BuiltinFn;
use std::alloc::Layout;
use std::ptr::NonNull;

/// Every native function of one compile, packed into a single mapping.
///
/// The region is reserved once and stays writable while functions are
/// appended; [`CodeArena::seal`] then makes it executable in one step, so the
/// code is never writable and executable at the same time. Entries handed
/// out by [`CodeArena::push`] point into the reservation, which never moves,
/// and are only called after sealing.
#[derive(Debug)]
pub struct CodeArena<P: Pages = PageAlloc> {
    ptr: NonNull<u8>,
    cap: usize,
    len: usize,
    sealed: bool,
    pages: P,
}

impl CodeArena {
    pub fn new() -> Result<Self, String> {
        Self::new_in(PageAlloc {})
    }
}

impl<P: Pages> CodeArena<P> {
    /// Pages are only backed once touched, so the reservation costs address
    /// space, not memory, as long as no huge page backs it, see
    /// [`CodeArena::new`]. 16 MiB fits hundreds of thousands of functions.
    const CAPACITY: usize = 16 << 20;
    /// Function entries start on 16 bytes so the decoder fetches a whole
    /// first block.
    const ALIGN: usize = 16;

    fn layout() -> Layout {
        Layout::from_size_align(Self::CAPACITY, mmap::page_size()).expect("code arena layout")
    }

    pub fn new_in(pages: P) -> Result<Self, String> {
        let ptr = pages
            .allocate(Self::layout())
            .map_err(|_| "mapping the code arena failed".to_string())?
            .cast::<u8>();
        // A few hundred bytes of code would otherwise take a whole 2 MiB huge
        // page. Without the advice, the arena still works.
        let _ = mmap::no_huge_pages(ptr, Self::CAPACITY);
        Ok(Self {
            ptr,
            cap: Self::CAPACITY,
            len: 0,
            sealed: false,
            pages,
        })
    }

    /// Copy `code` into the arena and return its entry, `None` once full.
    pub fn push(&mut self, code: &[u8]) -> Option<BuiltinFn> {
        debug_assert!(!self.sealed, "push into a sealed code arena");
        let start = self.len.next_multiple_of(Self::ALIGN);
        let end = start.checked_add(code.len())?;
        if end > self.cap {
            return None;
        }
        let base = self.ptr.as_ptr();
        unsafe {
            // int3 padding, so a stray fall-through traps.
            std::ptr::write_bytes(base.add(self.len), 0xcc, start - self.len);
            std::ptr::copy_nonoverlapping(code.as_ptr(), base.add(start), code.len());
        }
        self.len = end;
        Some(unsafe { std::mem::transmute::<*const u8, BuiltinFn>(base.add(start)) })
    }

    /// Make the written code executable. aarch64 will additionally need an
    /// instruction cache invalidation over `[ptr, ptr + len)` before this.
    pub fn seal(&mut self) -> Result<(), String> {
        self.sealed = true;
        if self.len == 0 {
            return Ok(());
        }
        mmap::mprotect(self.ptr, self.len, MmapProt::READ | MmapProt::EXEC)
    }
}

impl<P: Pages> Drop for CodeArena<P> {
    fn drop(&mut self) {
        unsafe { self.pages.deallocate(self.ptr, Self::layout()) };
    }
}
