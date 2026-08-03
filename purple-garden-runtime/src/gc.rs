use std::{alloc::Layout, ptr::NonNull};

use purple_garden_shared::mmap::{self, MmapFlags, MmapProt};

unsafe extern "C" {
    fn getpagesize() -> i32;
}

const METADATA_SIZE: usize = std::mem::size_of::<Metadata>();
const METADATA_ALIGN: usize = std::mem::align_of::<Metadata>();
const PAYLOAD_ALIGN: usize = std::mem::align_of::<u64>();
const _: () = assert!(PAYLOAD_ALIGN.is_multiple_of(METADATA_ALIGN));
/// Bits 0..2 of [`Metadata::flags`]: [`AllocType`] discriminant.
const TYPE_MASK: u8 = 0b111;
/// Bits 3..7 of [`Metadata::flags`]: collector state. Only [`MARKED_FLAG`] is
/// assigned so far.
const GCINFO_SHIFT: u32 = 3;
const GCINFO_MASK: u8 = 0b1_1111;
/// The low bit of the gcinfo field: mark/sweep liveness. Set during marking,
/// cleared after sweep.
const MARKED_FLAG: u8 = 1 << GCINFO_SHIFT;
/// Largest allocation a [`Layout`] can express. [`Metadata`] no longer
/// constrains this, its size field is a full `u64`.
pub const MAX_ALLOC_SIZE: usize = isize::MAX as usize;

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

    const fn bits(self) -> u8 {
        self as u8
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Per allocation metadata, filling the [`METADATA_SIZE`] bytes directly below
/// every payload:
///
/// ```text
/// payload-16          payload-8           payload
/// +-------------------+-------------------+
/// | size: u64         | flags: u8 + pad   |
/// +-------------------+-------------------+
///
/// flags: 7      3 2     0
///        +--------+------+
///        | gcinfo | type |
///        | 5 bits | 3bit |
///        +--------+------+
/// ```
pub struct Metadata {
    size: u64,
    flags: u8,
}

impl Metadata {
    #[must_use]
    pub fn new(alloc_type: AllocType, marked: bool, size: u64) -> Self {
        let marked = if marked { MARKED_FLAG } else { 0 };
        Self {
            size,
            flags: alloc_type.bits() | marked,
        }
    }

    #[must_use]
    pub unsafe fn from_payload(payload: NonNull<u8>) -> Self {
        unsafe { payload.as_ptr().sub(METADATA_SIZE).cast::<Self>().read() }
    }

    #[must_use]
    pub fn alloc_type(self) -> Option<AllocType> {
        AllocType::from_u8(self.flags & TYPE_MASK)
    }

    #[must_use]
    pub fn gcinfo(self) -> u8 {
        (self.flags >> GCINFO_SHIFT) & GCINFO_MASK
    }

    #[must_use]
    pub fn marked(self) -> bool {
        self.flags & MARKED_FLAG != 0
    }

    /// Payload size in bytes, excluding the metadata and any padding.
    #[must_use]
    pub fn size(self) -> u64 {
        self.size
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

    pub fn total_used(&self) -> usize {
        self.pages.iter().map(|p| p.len).sum()
    }

    pub fn total_alloc(&self) -> usize {
        self.pages.iter().map(|p| p.cap).sum()
    }

    pub fn alloc_fast(&mut self, alloc_type: AllocType, layout: Layout) -> Option<NonNull<u8>> {
        debug_assert!(layout.size() <= MAX_ALLOC_SIZE);
        debug_assert!(
            layout.align() <= PAYLOAD_ALIGN,
            "payload alignment is uniform; an over-aligned allocation would put \
             the metadata word out of reach of `payload - METADATA_SIZE`"
        );

        let metadata = Metadata::new(alloc_type, false, layout.size() as u64);
        let page = unsafe { self.pages.last_mut().unwrap_unchecked() };
        let base = page.ptr.as_ptr() as usize;
        let payload = align_up(base + page.len + METADATA_SIZE, PAYLOAD_ALIGN);
        let end = payload + layout.size() - base;

        if end > page.cap {
            return None;
        }

        unsafe {
            ((payload - METADATA_SIZE) as *mut Metadata).write(metadata);
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
        let required = METADATA_SIZE + PAYLOAD_ALIGN + layout.size();
        let page_size = align_up(required.max(self.page_size), self.page_size);
        self.pages.push(Page::new(page_size)?);
        Ok(())
    }
}

fn align_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (value + align - 1) & !(align - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(size: usize, align: usize) -> Layout {
        Layout::from_size_align(size, align).unwrap()
    }

    fn alloc_fast(
        gc: &mut Gc,
        alloc_type: AllocType,
        size: usize,
        align: usize,
    ) -> Option<NonNull<u8>> {
        gc.alloc_fast(alloc_type, layout(size, align))
    }

    #[test]
    fn allocations_are_aligned_and_do_not_overlap_their_metadata() {
        let mut gc = Gc::new();
        let mut spans = Vec::new();

        for size in [1, 7, 8, 9, 16, 31] {
            for align in [1, 2, 4, 8] {
                let payload = gc
                    .alloc_fast(AllocType::Record, layout(size, align))
                    .expect("page has room");
                let addr = payload.as_ptr() as usize;
                assert_eq!(addr % PAYLOAD_ALIGN, 0, "size={size} align={align}");
                spans.push((addr, size));
            }
        }

        spans.sort_unstable();
        for pair in spans.windows(2) {
            let ((addr, size), (next, _)) = (pair[0], pair[1]);
            assert!(
                addr + size + METADATA_SIZE <= next,
                "{addr:#x}+{size} runs into the metadata word below {next:#x}"
            );
        }
    }

    #[test]
    fn metadata_round_trips_through_the_payload_pointer() {
        let mut gc = Gc::new();
        let requested = [
            (AllocType::Record, 16),
            (AllocType::String, 24),
            (AllocType::Array, 8),
            (AllocType::Option, 1),
        ];

        let payloads =
            requested.map(|(alloc_type, size)| alloc_fast(&mut gc, alloc_type, size, 8).unwrap());

        for ((alloc_type, size), payload) in requested.iter().zip(payloads) {
            let metadata = unsafe { Metadata::from_payload(payload) };
            assert_eq!(metadata, Metadata::new(*alloc_type, false, *size as u64));
            assert_eq!(metadata.alloc_type(), Some(*alloc_type));
            assert_eq!(metadata.size(), *size as u64);
            assert_eq!(metadata.gcinfo(), 0);
            assert!(!metadata.marked());
        }
    }

    #[test]
    fn alloc_fast_reports_a_full_page_instead_of_spilling() {
        let mut gc = Gc::new();
        let page_size = gc.page_size;
        // A fresh page spends its first METADATA_SIZE bytes on the metadata.
        let exact_fit = page_size - METADATA_SIZE;

        assert!(alloc_fast(&mut gc, AllocType::String, exact_fit + 1, 8).is_none());
        assert!(alloc_fast(&mut gc, AllocType::String, exact_fit, 8).is_some());
        assert!(alloc_fast(&mut gc, AllocType::String, 1, 8).is_none());
    }

    #[test]
    fn grow_serves_allocations_larger_than_a_page() {
        let mut gc = Gc::new();
        let oversized = gc.page_size * 3;

        assert!(alloc_fast(&mut gc, AllocType::Array, oversized, 8).is_none());
        gc.grow(layout(oversized, 8)).expect("mmap a larger page");

        let payload = alloc_fast(&mut gc, AllocType::Array, oversized, 8).expect("fresh page fits");
        assert_eq!(payload.as_ptr() as usize % PAYLOAD_ALIGN, 0);
        assert_eq!(
            unsafe { Metadata::from_payload(payload) },
            Metadata::new(AllocType::Array, false, oversized as u64)
        );
        assert!(gc.total_alloc() > gc.total_used());
        assert!(gc.total_used() >= oversized);
    }
}
