use std::{alloc::Layout, ptr, ptr::NonNull};

use purple_garden_shared::mmap::{self, MmapFlags, MmapProt};

use crate::Value;

unsafe extern "C" {
    fn getpagesize() -> i32;
}

const METADATA_SIZE: usize = std::mem::size_of::<u32>();
const METADATA_ALIGN: usize = std::mem::align_of::<u32>();

/// Bit 3: mark/sweep liveness bit. Set during marking, cleared after sweep.
const MARKED_FLAG: u32 = 1 << 3;
/// Bits 8..31: payload allocation size in bytes.
const SIZE_SHIFT: u32 = 8;
/// The metadata word reserves 24 bits for payload allocation size.
const MAX_ALLOC_SIZE: usize = 0x00ff_ffff;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AllocType {
    Record = 0,
    Array = 1,
    Option = 2,
    String = 3,
}

impl AllocType {
    const fn bits(self) -> u32 {
        self as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// One word before every allocation payload:
///
/// ```text
/// 31       8 7      4 3      2       0
/// +----------+--------+--------+-------+
/// | size     | flags  | marked | type  |
/// | 24 bits  | 4 bits | 1 bit  | 3 bit |
/// +----------+--------+--------+-------+
/// ```
pub struct Metadata(u32);

impl Metadata {
    #[must_use]
    pub fn new(alloc_type: AllocType, marked: bool, size: u32) -> Self {
        let marked = if marked { MARKED_FLAG } else { 0 };
        Self(alloc_type.bits() | marked | (size << SIZE_SHIFT))
    }

    #[must_use]
    pub fn bits(self) -> u32 {
        self.0
    }
}

#[derive(Debug)]
struct Page {
    ptr: NonNull<u8>,
    cap: usize,
    len: usize,
}

impl Page {
    fn new(cap: usize) -> Self {
        let ptr = mmap::mmap(
            None,
            cap,
            MmapProt::READ | MmapProt::WRITE,
            MmapFlags::PRIVATE | MmapFlags::ANONYMOUS,
            -1,
            0,
        )
        .expect("anonymous GC page mmap");

        Self { ptr, cap, len: 0 }
    }

    fn alloc(&mut self, metadata: Metadata, layout: Layout) -> Option<NonNull<u8>> {
        let base = self.ptr.as_ptr() as usize;
        let metadata_addr = align_up(base.checked_add(self.len)?, METADATA_ALIGN);
        let payload = metadata_addr
            .checked_add(METADATA_SIZE)
            .map(|addr| align_up(addr, layout.align()))?;
        let end = payload.checked_add(layout.size())?.checked_sub(base)?;
        if end > self.cap {
            return None;
        }

        unsafe {
            (metadata_addr as *mut u32).write(metadata.bits());
        }
        self.len = end;

        Some(unsafe { NonNull::new_unchecked(payload as *mut u8) })
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        mmap::munmap(self.ptr, self.cap).expect("GC page munmap");
    }
}

#[derive(Debug)]
pub struct Gc {
    pages: Vec<Page>,
    page_size: usize,
}

impl Default for Gc {
    fn default() -> Self {
        let page_size = unsafe { getpagesize() };
        assert!(page_size > 0, "getpagesize returned {page_size}");
        Self {
            pages: Vec::new(),
            page_size: page_size as usize,
        }
    }
}

impl Gc {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn alloc(&mut self, alloc_type: AllocType, layout: Layout) -> NonNull<u8> {
        assert!(layout.size() <= MAX_ALLOC_SIZE);

        let metadata = Metadata::new(alloc_type, false, layout.size() as u32);
        let required = METADATA_SIZE + layout.align() + layout.size();
        let page_size = align_up(required.max(self.page_size), self.page_size);

        if let Some(page) = self.pages.last_mut()
            && let Some(payload) = page.alloc(metadata, layout)
        {
            #[cfg(feature = "trace_gc")]
            purple_garden_shared::trace!(
                "[gc::alloc] type={:?} size={} align={} payload={:#x} page_used={}/{}",
                alloc_type,
                layout.size(),
                layout.align(),
                payload.as_ptr() as usize,
                page.len,
                page.cap,
            );

            return payload;
        }

        self.pages.push(Page::new(page_size));
        let page = self.pages.last_mut().expect("page was just inserted");
        let payload = page
            .alloc(metadata, layout)
            .expect("new page must fit allocation");

        #[cfg(feature = "trace_gc")]
        purple_garden_shared::trace!(
            "[gc::alloc] type={:?} size={} align={} payload={:#x} page_used={}/{}",
            alloc_type,
            layout.size(),
            layout.align(),
            payload.as_ptr() as usize,
            page.len,
            page.cap,
        );

        payload
    }

    pub fn alloc_string(&mut self, bytes: &[u8]) -> Value {
        let len_size = std::mem::size_of::<usize>();
        let layout = Layout::from_size_align(len_size + bytes.len(), std::mem::align_of::<usize>())
            .expect("string allocation layout");
        let payload = self.alloc(AllocType::String, layout).as_ptr();

        unsafe {
            (payload as *mut usize).write(bytes.len());
            ptr::copy_nonoverlapping(bytes.as_ptr(), payload.add(len_size), bytes.len());
        }

        Value::from_ptr(payload)
    }
}

fn align_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (value + align - 1) & !(align - 1)
}
