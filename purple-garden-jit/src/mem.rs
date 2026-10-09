//! rwx page allocator for JIT'd code. Uses the syscall wrappers from
//! [`purple_garden_allocators::page::mmap`].

use purple_garden_runtime::BuiltinFn;
use purple_garden_allocators::page::mmap::{self, MmapFlags, MmapProt};
use std::ptr::NonNull;

#[derive(Debug)]
pub struct ExecPage {
    ptr: NonNull<u8>,
    len: usize,
}

impl ExecPage {
    pub fn new(code: &[u8]) -> Result<Self, String> {
        let len = code.len();

        let ptr = mmap::mmap(
            None,
            len,
            MmapProt::READ | MmapProt::WRITE,
            MmapFlags::PRIVATE | MmapFlags::ANONYMOUS,
            -1,
            0,
        )?;

        unsafe { std::ptr::copy_nonoverlapping(code.as_ptr(), ptr.as_ptr(), len) };

        if let Err(e) = mmap::mprotect(ptr, len, MmapProt::READ | MmapProt::EXEC) {
            // give the kernel back the page before bailing
            let _ = mmap::munmap(ptr, len);
            return Err(e);
        }

        Ok(Self { ptr, len })
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }
}

impl Drop for ExecPage {
    fn drop(&mut self) {
        let _ = mmap::munmap(self.ptr, self.len);
    }
}

/// Every native function of one compile, packed into a single mapping.
///
/// The region is reserved once and stays writable while functions are
/// appended; [`CodeArena::seal`] then makes it executable in one step, so the
/// code is never writable and executable at the same time. Entries handed
/// out by [`CodeArena::push`] point into the reservation, which never moves,
/// and are only called after sealing.
#[derive(Debug)]
pub struct CodeArena {
    ptr: NonNull<u8>,
    cap: usize,
    len: usize,
    sealed: bool,
}

impl CodeArena {
    /// Pages are only backed once touched, so the reservation costs address
    /// space, not memory, as long as no huge page backs it, see
    /// [`CodeArena::new`]. 16 MiB fits hundreds of thousands of functions.
    const CAPACITY: usize = 16 << 20;
    /// Function entries start on 16 bytes so the decoder fetches a whole
    /// first block.
    const ALIGN: usize = 16;

    pub fn new() -> Result<Self, String> {
        let ptr = mmap::mmap(
            None,
            Self::CAPACITY,
            MmapProt::READ | MmapProt::WRITE,
            MmapFlags::PRIVATE | MmapFlags::ANONYMOUS,
            -1,
            0,
        )?;
        // A few hundred bytes of code would otherwise take a whole 2 MiB huge
        // page. Without the advice, the arena still works.
        let _ = mmap::no_huge_pages(ptr, Self::CAPACITY);
        Ok(Self {
            ptr,
            cap: Self::CAPACITY,
            len: 0,
            sealed: false,
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

impl Drop for CodeArena {
    fn drop(&mut self) {
        let _ = mmap::munmap(self.ptr, self.cap);
    }
}
