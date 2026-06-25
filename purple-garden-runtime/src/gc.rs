use std::{alloc::Layout, ptr::NonNull};

use purple_garden_shared::mmap::{self, MmapFlags, MmapProt};

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
pub const MAX_ALLOC_SIZE: usize = 0x00ff_ffff;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AllocType {
    Record = 0,
    Array = 1,
    Option = 2,
    String = 3,
}

impl AllocType {
    pub fn from_ty(value: &purple_garden_ir::ptype::Type<'_>) -> Option<Self> {
        Some(match value {
            purple_garden_ir::ptype::Type::Str => Self::String,
            purple_garden_ir::ptype::Type::Option(_) => Self::Option,
            purple_garden_ir::ptype::Type::Array(_) => Self::Array,
            purple_garden_ir::ptype::Type::Record(_) => Self::Record,
            _ => return None,
        })
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Record,
            1 => Self::Array,
            2 => Self::Option,
            3 => Self::String,
            _ => return None,
        })
    }

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
    fn new(cap: usize) -> Result<Self, String> {
        let ptr = mmap::mmap(
            None,
            cap,
            MmapProt::READ | MmapProt::WRITE,
            MmapFlags::PRIVATE | MmapFlags::ANONYMOUS,
            -1,
            0,
        )?;

        Ok(Self { ptr, cap, len: 0 })
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
            pages: vec![Page::new(page_size as usize).expect("anonymous GC page mmap")],
            page_size: page_size as usize,
        }
    }
}

impl Gc {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn collect(&mut self) {
        // Tracing/sweeping will be wired here once the VM passes roots through.
    }

    pub fn alloc_fast(&mut self, alloc_type: AllocType, layout: Layout) -> Option<NonNull<u8>> {
        debug_assert!(layout.size() <= MAX_ALLOC_SIZE);

        let metadata = Metadata::new(alloc_type, false, layout.size() as u32);
        let page = unsafe { self.pages.last_mut().unwrap_unchecked() };
        let base = page.ptr.as_ptr() as usize;
        let metadata_addr = align_up(base + page.len, METADATA_ALIGN);
        let payload = align_up(metadata_addr + METADATA_SIZE, layout.align());
        let end = payload + layout.size() - base;

        if end > page.cap {
            return None;
        }

        unsafe {
            (metadata_addr as *mut u32).write(metadata.bits());
        }
        page.len = end;

        let payload = unsafe { NonNull::new_unchecked(payload as *mut u8) };

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

        Some(payload)
    }

    pub fn grow(&mut self, layout: Layout) -> Result<(), String> {
        let required = METADATA_SIZE + layout.align() + layout.size();
        let page_size = align_up(required.max(self.page_size), self.page_size);
        self.pages.push(Page::new(page_size)?);
        Ok(())
    }
}

fn align_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (value + align - 1) & !(align - 1)
}
