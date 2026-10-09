//! Memory straight from the OS, see [`PageAlloc`].

pub mod mmap;

use std::alloc::{AllocError, Allocator, Layout};
use std::ptr::NonNull;

use mmap::{MmapFlags, MmapProt};

/// Maps every allocation as its own run of whole pages. Fresh pages are
/// zeroed and only become resident once touched, so a large allocation costs
/// address space until it is used.
#[derive(Debug, Default, Clone, Copy)]
pub struct PageAlloc {}

/// The bytes a mapping for `layout` spans; `deallocate` unmaps the same run.
/// Allocators whose every allocation is its own run of whole pages, so its
/// protection can be changed with [`mmap::mprotect`] without touching anything
/// else, e.g. to make JIT'd code executable.
///
/// # Safety
///
/// Every non-empty allocation must start on a page and own all pages it spans.
pub unsafe trait Pages: Allocator {}

unsafe impl Pages for PageAlloc {}
unsafe impl<P: Pages> Pages for &P {}

fn mapped_len(layout: Layout) -> usize {
    layout.size().next_multiple_of(mmap::page_size())
}

unsafe impl Allocator for PageAlloc {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        if layout.size() == 0 {
            return Ok(NonNull::slice_from_raw_parts(layout.dangling_ptr(), 0));
        }
        let len = mapped_len(layout);
        let page = mmap::page_size();
        // Mappings start on a page; a larger alignment maps the slack in front
        // and returns the unaligned head and the tail behind it.
        let slack = layout.align().saturating_sub(page);
        let map = mmap::mmap(
            None,
            len + slack,
            MmapProt::READ | MmapProt::WRITE,
            MmapFlags::PRIVATE | MmapFlags::ANONYMOUS,
            -1,
            0,
        )
        .map_err(|_| AllocError)?;
        let head = map.as_ptr().align_offset(layout.align());
        let ptr = unsafe { map.add(head) };
        if head > 0 {
            mmap::munmap(map, head).map_err(|_| AllocError)?;
        }
        if slack > head {
            mmap::munmap(unsafe { ptr.add(len) }, slack - head).map_err(|_| AllocError)?;
        }
        Ok(NonNull::slice_from_raw_parts(ptr, len))
    }

    /// Fresh mappings are already zero: the default would write every page
    /// and make it resident.
    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if layout.size() != 0 {
            mmap::munmap(ptr, mapped_len(layout)).expect("unmapping pages of a live allocation");
        }
    }

    /// Remaps instead of copying: pages already touched stay resident, only the new ones fault.
    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        if old.size() == 0 {
            return self.allocate(new);
        }
        let (old_len, new_len) = (mapped_len(old), mapped_len(new));
        if new_len == old_len && ptr.addr().get().is_multiple_of(new.align()) {
            return Ok(NonNull::slice_from_raw_parts(ptr, old_len));
        }
        #[cfg(target_os = "linux")]
        if new.align() <= mmap::page_size() {
            let moved = mmap::mremap(ptr, old_len, new_len).map_err(|_| AllocError)?;
            return Ok(NonNull::slice_from_raw_parts(moved, new_len));
        }
        let block = self.allocate(new)?;
        unsafe {
            ptr.copy_to_nonoverlapping(block.cast(), old.size());
            self.deallocate(ptr, old);
        }
        Ok(block)
    }

    /// Only the bytes the old allocation may have written are zeroed, remapped pages are fresh.
    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        if old.size() != 0 {
            unsafe {
                ptr.add(old.size())
                    .write_bytes(0, mapped_len(old).min(new.size()) - old.size());
            }
        }
        unsafe { self.grow(ptr, old, new) }
    }

    /// Unmaps the pages past the new end.
    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        if !ptr.addr().get().is_multiple_of(new.align()) {
            let block = self.allocate(new)?;
            unsafe {
                ptr.copy_to_nonoverlapping(block.cast(), new.size());
                self.deallocate(ptr, old);
            }
            return Ok(block);
        }
        if new.size() == 0 {
            unsafe { self.deallocate(ptr, old) };
            return Ok(NonNull::slice_from_raw_parts(new.dangling_ptr(), 0));
        }
        let (old_len, new_len) = (mapped_len(old), mapped_len(new));
        if new_len < old_len {
            mmap::munmap(unsafe { ptr.add(new_len) }, old_len - new_len).map_err(|_| AllocError)?;
        }
        Ok(NonNull::slice_from_raw_parts(ptr, new_len))
    }
}

#[cfg(test)]
mod tests {
    use super::{PageAlloc, mmap};
    use std::alloc::{Allocator, Layout};

    #[test]
    fn allocations_span_whole_zeroed_pages() {
        let page = mmap::page_size();
        let layout = Layout::from_size_align(page + 1, 8).unwrap();
        let block = PageAlloc {}.allocate_zeroed(layout).unwrap();
        assert_eq!(block.len(), 2 * page);
        let bytes = unsafe { block.as_ref() };
        assert!(bytes.iter().all(|&b| b == 0));
        unsafe { PageAlloc {}.deallocate(block.cast(), layout) };
    }

    #[test]
    fn honours_alignment_beyond_a_page() {
        let align = 16 * mmap::page_size();
        let layout = Layout::from_size_align(100, align).unwrap();
        let block = PageAlloc {}.allocate(layout).unwrap();
        assert_eq!(block.cast::<u8>().as_ptr() as usize % align, 0);
        unsafe {
            block.cast::<u8>().write_bytes(0xab, 100);
            PageAlloc {}.deallocate(block.cast(), layout);
        }
    }

    /// Resident pages in `[ptr, ptr + len)`, see `mincore(2)`. Unlike the
    /// process RSS it doesn't see what tests running in parallel touch.
    #[cfg(target_os = "linux")]
    fn resident_pages(ptr: std::ptr::NonNull<u8>, len: usize) -> usize {
        unsafe extern "C" {
            fn mincore(addr: *mut std::ffi::c_void, len: usize, vec: *mut u8) -> i32;
        }
        let mut pages = vec![0u8; len.div_ceil(mmap::page_size())];
        assert_eq!(
            unsafe { mincore(ptr.as_ptr().cast(), len, pages.as_mut_ptr()) },
            0
        );
        pages.iter().filter(|&&page| page & 1 != 0).count()
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn zeroed_allocations_stay_untouched() {
        let layout = Layout::from_size_align(64 << 20, 8).unwrap();
        let block = PageAlloc {}.allocate_zeroed(layout).unwrap();
        let resident = resident_pages(block.cast(), block.len());
        unsafe { block.cast::<u8>().write(1) };
        let touched = resident_pages(block.cast(), block.len());
        unsafe { PageAlloc {}.deallocate(block.cast(), layout) };
        assert_eq!(resident, 0);
        assert!(touched >= 1);
    }

    #[test]
    fn zero_sized_allocations_map_nothing() {
        let layout = Layout::from_size_align(0, 64).unwrap();
        let block = PageAlloc {}.allocate(layout).unwrap();
        assert_eq!(block.len(), 0);
        assert_eq!(block.cast::<u8>().as_ptr() as usize % 64, 0);
        unsafe { PageAlloc {}.deallocate(block.cast(), layout) };
    }

    #[test]
    fn grows_and_shrinks_keeping_contents() {
        let page = mmap::page_size();
        let mut values: Vec<u64, _> = Vec::with_capacity_in(16, PageAlloc {});
        for i in 0..(64 * page as u64) {
            values.push(i);
        }
        assert!(values.iter().copied().eq(0..(64 * page as u64)));
        values.truncate(10);
        values.shrink_to_fit();
        assert!(values.iter().copied().eq(0..10));
    }

    #[test]
    fn grown_zeroed_memory_is_zero_after_writes() {
        let page = mmap::page_size();
        let (old, new) = (
            Layout::from_size_align(10, 8).unwrap(),
            Layout::from_size_align(3 * page, 8).unwrap(),
        );
        let block = PageAlloc {}.allocate(old).unwrap();
        unsafe { block.cast::<u8>().write_bytes(0xff, block.len()) };
        let grown = unsafe { PageAlloc {}.grow_zeroed(block.cast(), old, new) }.unwrap();
        let bytes = unsafe { grown.as_ref() };
        assert!(bytes[..10].iter().all(|&b| b == 0xff));
        assert!(bytes[10..].iter().all(|&b| b == 0));
        unsafe { PageAlloc {}.deallocate(grown.cast(), new) };
    }

    #[test]
    fn backs_collections() {
        let mut values = Vec::with_capacity_in(1000, PageAlloc {});
        values.extend(0..1000u64);
        values.extend(1000..5000);
        assert_eq!(values.iter().sum::<u64>(), 4999 * 5000 / 2);
        let boxed = Box::new_in([7u8; 64], PageAlloc {});
        assert_eq!(boxed[63], 7);
    }
}
