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

    /// Resident pages of this process, see `proc_pid_statm(5)`.
    #[cfg(target_os = "linux")]
    fn resident_pages() -> usize {
        let statm = std::fs::read_to_string("/proc/self/statm").unwrap();
        statm.split_whitespace().nth(1).unwrap().parse().unwrap()
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn zeroed_allocations_stay_untouched() {
        let layout = Layout::from_size_align(64 << 20, 8).unwrap();
        let before = resident_pages();
        let block = PageAlloc {}.allocate_zeroed(layout).unwrap();
        let grown = (resident_pages() - before) * mmap::page_size();
        unsafe { PageAlloc {}.deallocate(block.cast(), layout) };
        assert!(grown < 1 << 20, "{grown} bytes became resident");
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
    fn backs_collections() {
        let mut values = Vec::with_capacity_in(1000, PageAlloc {});
        values.extend(0..1000u64);
        values.extend(1000..5000);
        assert_eq!(values.iter().sum::<u64>(), 4999 * 5000 / 2);
        let boxed = Box::new_in([7u8; 64], PageAlloc {});
        assert_eq!(boxed[63], 7);
    }
}
