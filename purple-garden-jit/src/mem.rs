//! rwx page allocator for JIT'd code. Uses the syscall wrappers from
//! [`purple_garden_shared::mmap`].

use purple_garden_shared::mmap::{self, MmapFlags, MmapProt};
use std::ptr::NonNull;

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
