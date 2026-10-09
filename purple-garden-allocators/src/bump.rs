//! Arena allocations out of growing chunks, see [`BumpAlloc`].

use std::alloc::{AllocError, Allocator, Layout};
use std::cell::Cell;
use std::ptr::NonNull;

use crate::metric::MetricAlloc;
use crate::page::PageAlloc;

/// Allocators whose memory stays valid until the allocator itself is dropped,
/// which frees all of it. Collections can be leaked into an arena: the
/// returned `&'a mut [T]` lives as long as the borrow of the arena.
///
/// # Safety
///
/// Memory that is never deallocated must stay valid until the arena is reset
/// through `&mut` or dropped, and dropping must reclaim it.
pub unsafe trait Arena: Allocator {
    /// Moves `value` into the arena, it is never dropped.
    fn alloc<T>(&self, value: T) -> &mut T {
        Box::leak(Box::new_in(value, self))
    }

    /// Moves `items` into the arena, they are never dropped.
    fn alloc_slice<T>(&self, items: impl IntoIterator<Item = T>) -> &mut [T] {
        let mut v = Vec::new_in(self);
        v.extend(items);
        v.leak()
    }
}

unsafe impl<A: Arena> Arena for &A {}
unsafe impl<A: Arena> Arena for MetricAlloc<A> {}

const FIRST_CHUNK: usize = 64 << 10;
const MAX_CHUNK: usize = 16 << 20;

/// Lives at the start of every chunk, linking it to the one before.
struct Chunk {
    prev: Option<NonNull<Chunk>>,
    layout: Layout,
}

/// Hands out memory by bumping a pointer through chunks taken from `A`.
/// Chunks start at 64 KiB and double up to 16 MiB, a request that doesn't
/// fit one gets a chunk of its own size.
///
/// Freeing only takes back the most recent allocation, and only that one
/// grows in place, everything else stays until [`BumpAlloc::reset`] or drop.
pub struct BumpAlloc<A: Allocator = PageAlloc> {
    parent: A,
    chunk: Cell<Option<NonNull<Chunk>>>,
    top: Cell<NonNull<u8>>,
    end: Cell<NonNull<u8>>,
    next: Cell<usize>,
}

// It owns its chunks, nothing else points into them without borrowing it.
unsafe impl<A: Allocator + Send> Send for BumpAlloc<A> {}

unsafe impl<A: Allocator> Arena for BumpAlloc<A> {}

impl Default for BumpAlloc {
    fn default() -> Self {
        Self::new_in(PageAlloc {})
    }
}

impl BumpAlloc {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<A: Allocator> BumpAlloc<A> {
    pub const fn new_in(parent: A) -> Self {
        Self {
            parent,
            chunk: Cell::new(None),
            top: Cell::new(NonNull::dangling()),
            end: Cell::new(NonNull::dangling()),
            next: Cell::new(FIRST_CHUNK),
        }
    }

    /// Frees everything but the newest chunk and starts over at its
    /// beginning.
    pub fn reset(&mut self) {
        let Some(chunk) = self.chunk.get() else {
            return;
        };
        unsafe {
            self.free_from((*chunk.as_ptr()).prev.take());
            self.top.set(Self::start(chunk));
        }
    }

    fn start(chunk: NonNull<Chunk>) -> NonNull<u8> {
        unsafe { chunk.add(1).cast() }
    }

    unsafe fn free_from(&self, mut chunk: Option<NonNull<Chunk>>) {
        while let Some(c) = chunk {
            let Chunk { prev, layout } = unsafe { c.read() };
            unsafe { self.parent.deallocate(c.cast(), layout) };
            chunk = prev;
        }
    }

    /// Where `layout` starts and ends in the current chunk, if it fits.
    fn fit(&self, layout: Layout) -> Option<(NonNull<u8>, NonNull<u8>)> {
        let top = self.top.get();
        let pad = top.align_offset(layout.align());
        let room = self.end.get().addr().get() - top.addr().get();
        if pad.checked_add(layout.size())? > room {
            return None;
        }
        let ptr = unsafe { top.add(pad) };
        Some((ptr, unsafe { ptr.add(layout.size()) }))
    }

    #[cold]
    fn grow_chunks(&self, layout: Layout) -> Result<(), AllocError> {
        let header = Layout::new::<Chunk>();
        let need = header.size().next_multiple_of(layout.align()) + layout.size();
        let size = self.next.get().max(need);
        let chunk_layout = Layout::from_size_align(size, header.align().max(layout.align()))
            .map_err(|_| AllocError)?;
        let block = self.parent.allocate(chunk_layout)?;
        let chunk = block.cast::<Chunk>();
        unsafe {
            chunk.write(Chunk {
                prev: self.chunk.get(),
                layout: chunk_layout,
            })
        };
        self.chunk.set(Some(chunk));
        self.top.set(Self::start(chunk));
        self.end.set(unsafe { block.cast::<u8>().add(block.len()) });
        self.next.set((self.next.get() * 2).min(MAX_CHUNK));
        Ok(())
    }

    fn is_top(&self, ptr: NonNull<u8>, layout: Layout) -> bool {
        unsafe { ptr.add(layout.size()) == self.top.get() }
    }
}

unsafe impl<A: Allocator> Allocator for BumpAlloc<A> {
    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        if layout.size() == 0 {
            return Ok(NonNull::slice_from_raw_parts(layout.dangling_ptr(), 0));
        }
        let (ptr, end) = match self.fit(layout) {
            Some(fit) => fit,
            None => {
                self.grow_chunks(layout)?;
                self.fit(layout).ok_or(AllocError)?
            }
        };
        self.top.set(end);
        Ok(NonNull::slice_from_raw_parts(ptr, layout.size()))
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if self.is_top(ptr, layout) {
            self.top.set(ptr);
        }
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        if self.is_top(ptr, old) && ptr.addr().get().is_multiple_of(new.align()) {
            let end = unsafe { ptr.add(new.size()) };
            if end <= self.end.get() {
                self.top.set(end);
                return Ok(NonNull::slice_from_raw_parts(ptr, new.size()));
            }
        }
        let block = self.allocate(new)?;
        unsafe { ptr.copy_to_nonoverlapping(block.cast(), old.size()) };
        Ok(block)
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        if !ptr.addr().get().is_multiple_of(new.align()) {
            let block = self.allocate(new)?;
            unsafe { ptr.copy_to_nonoverlapping(block.cast(), new.size()) };
            return Ok(block);
        }
        if self.is_top(ptr, old) {
            self.top.set(unsafe { ptr.add(new.size()) });
        }
        Ok(NonNull::slice_from_raw_parts(ptr, new.size()))
    }
}

impl<A: Allocator> Drop for BumpAlloc<A> {
    fn drop(&mut self) {
        unsafe { self.free_from(self.chunk.get()) };
    }
}

impl<A: Allocator> std::fmt::Debug for BumpAlloc<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BumpAlloc")
            .field("next", &self.next.get())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::{Arena, BumpAlloc, FIRST_CHUNK};
    use crate::metric::MetricAlloc;
    use crate::page::PageAlloc;
    use std::alloc::{Allocator, Layout};

    fn layout(size: usize, align: usize) -> Layout {
        Layout::from_size_align(size, align).unwrap()
    }

    #[test]
    fn allocations_bump_aligned_through_one_chunk() {
        let bump = BumpAlloc::new();
        let a = bump.allocate(layout(3, 1)).unwrap().cast::<u8>();
        let b = bump.allocate(layout(8, 8)).unwrap().cast::<u8>();
        let c = bump.allocate(layout(1, 1)).unwrap().cast::<u8>();
        assert!(b.addr().get().is_multiple_of(8));
        assert!(b > a && b.addr().get() - a.addr().get() < 16);
        assert_eq!(unsafe { b.add(8) }, c);
    }

    #[test]
    fn freeing_the_top_allocation_reuses_it() {
        let bump = BumpAlloc::new();
        let l = layout(32, 8);
        let a = bump.allocate(l).unwrap().cast::<u8>();
        let b = bump.allocate(l).unwrap().cast::<u8>();
        unsafe { bump.deallocate(a, l) };
        unsafe { bump.deallocate(b, l) };
        assert_eq!(bump.allocate(l).unwrap().cast::<u8>(), b);
    }

    #[test]
    fn the_top_allocation_grows_in_place() {
        let bump = BumpAlloc::new();
        let mut v = Vec::with_capacity_in(1, &bump);
        v.push(0u64);
        let start = v.as_ptr();
        v.extend(1..1000);
        assert_eq!(v.as_ptr(), start);
        assert!(v.iter().copied().eq(0..1000));
    }

    #[test]
    fn interleaved_vecs_keep_their_contents() {
        let bump = BumpAlloc::new();
        let mut a = Vec::new_in(&bump);
        let mut b = Vec::new_in(&bump);
        for i in 0..10_000u32 {
            a.push(i);
            b.push(u64::from(i) * 3);
        }
        assert!(a.iter().copied().eq(0..10_000));
        assert!(b.iter().copied().eq((0..10_000).map(|i| i * 3)));
    }

    #[test]
    fn oversized_and_overaligned_requests_get_their_own_chunk() {
        let bump = BumpAlloc::new();
        let big = bump.allocate(layout(4 * FIRST_CHUNK, 8)).unwrap();
        assert_eq!(big.len(), 4 * FIRST_CHUNK);
        let page = bump.allocate(layout(16, 1 << 16)).unwrap().cast::<u8>();
        assert!(page.addr().get().is_multiple_of(1 << 16));
    }

    #[test]
    fn zero_sized_allocations_are_aligned() {
        let bump = BumpAlloc::new();
        let p = bump.allocate(layout(0, 64)).unwrap().cast::<u8>();
        assert!(p.addr().get().is_multiple_of(64));
    }

    #[test]
    fn chunks_go_back_to_the_parent() {
        let parent = MetricAlloc::new(PageAlloc {});
        let mut bump = BumpAlloc::new_in(&parent);
        for _ in 0..1000 {
            bump.alloc_slice([0u8; 1024]);
        }
        let grown = parent.metrics();
        assert!(grown.allocs > 1);
        bump.reset();
        let reset = parent.metrics();
        assert_eq!(reset.allocs - reset.frees, 1);
        assert!(reset.live < grown.live);
        drop(bump);
        assert_eq!(parent.metrics().live, 0);
    }

    #[test]
    fn reset_reuses_the_newest_chunk() {
        let parent = MetricAlloc::new(PageAlloc {});
        let mut bump = BumpAlloc::new_in(&parent);
        let first = bump.alloc_slice([1u32; 8]).as_ptr().cast::<u8>();
        bump.reset();
        let again = bump.alloc_slice([2u32; 8]).as_ptr().cast::<u8>();
        assert_eq!(first, again);
        assert_eq!(parent.metrics().allocs, 1);
    }

    #[test]
    fn slices_live_as_long_as_the_arena() {
        let bump = BumpAlloc::new();
        let ids: Vec<&[usize]> = (0..100).map(|n| &*bump.alloc_slice([n; 3])).collect();
        assert!(ids.iter().enumerate().all(|(n, s)| s == &[n; 3]));
    }
}
