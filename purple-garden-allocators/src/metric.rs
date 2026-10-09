//! Counting allocator actions does, see [`MetricAlloc`].

use std::alloc::{AllocError, Allocator, Global, Layout};
use std::cell::Cell;
use std::ptr::NonNull;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Metrics {
    pub allocs: usize,
    pub frees: usize,
    pub resizes: usize,
    pub bytes: usize,
    pub live: usize,
    pub peak: usize,
}

/// Forwards to `inner` and counts every call.
#[derive(Debug, Default)]
pub struct MetricAlloc<A = Global> {
    inner: A,
    metrics: Cell<Metrics>,
}

impl<A> MetricAlloc<A> {
    pub const fn new(inner: A) -> Self {
        Self {
            inner,
            metrics: Cell::new(Metrics {
                allocs: 0,
                frees: 0,
                resizes: 0,
                bytes: 0,
                live: 0,
                peak: 0,
            }),
        }
    }

    pub fn metrics(&self) -> Metrics {
        self.metrics.get()
    }

    pub fn into_inner(self) -> A {
        self.inner
    }

    fn record(&self, f: impl FnOnce(&mut Metrics)) {
        let mut m = self.metrics.get();
        f(&mut m);
        m.peak = m.peak.max(m.live);
        self.metrics.set(m);
    }

    fn allocated(&self, size: usize) {
        self.record(|m| {
            m.allocs += 1;
            m.bytes += size;
            m.live += size;
        });
    }

    fn resized(&self, old: usize, new: usize) {
        self.record(|m| {
            m.resizes += 1;
            m.bytes += new.saturating_sub(old);
            m.live = m.live - old + new;
        });
    }
}

unsafe impl<A: Allocator> Allocator for MetricAlloc<A> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let block = self.inner.allocate(layout)?;
        self.allocated(layout.size());
        Ok(block)
    }

    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let block = self.inner.allocate_zeroed(layout)?;
        self.allocated(layout.size());
        Ok(block)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe { self.inner.deallocate(ptr, layout) };
        self.record(|m| {
            m.frees += 1;
            m.live -= layout.size();
        });
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        let block = unsafe { self.inner.grow(ptr, old, new) }?;
        self.resized(old.size(), new.size());
        Ok(block)
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        let block = unsafe { self.inner.grow_zeroed(ptr, old, new) }?;
        self.resized(old.size(), new.size());
        Ok(block)
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        let block = unsafe { self.inner.shrink(ptr, old, new) }?;
        self.resized(old.size(), new.size());
        Ok(block)
    }
}

#[cfg(test)]
mod tests {
    use super::{MetricAlloc, Metrics};
    use crate::page::PageAlloc;

    #[test]
    fn counts_a_vec_through_its_lifetime() {
        let alloc = MetricAlloc::new(std::alloc::Global);
        let mut v = Vec::with_capacity_in(4, &alloc);
        v.extend(0u64..4);
        v.push(4);
        v.shrink_to_fit();
        drop(v);
        assert_eq!(
            alloc.metrics(),
            Metrics {
                allocs: 1,
                frees: 1,
                resizes: 2,
                bytes: 64,
                live: 0,
                peak: 64,
            }
        );
    }

    #[test]
    fn peak_outlives_frees() {
        let alloc = MetricAlloc::new(PageAlloc {});
        let a = Box::new_in([0u8; 100], &alloc);
        let b = Box::new_in([0u8; 50], &alloc);
        drop(a);
        let m = alloc.metrics();
        assert_eq!((m.live, m.peak), (50, 150));
        drop(b);
        assert_eq!(alloc.metrics().live, 0);
    }

    #[test]
    fn failed_allocations_are_not_counted() {
        use std::alloc::{Allocator, Layout};
        let alloc = MetricAlloc::new(PageAlloc {});
        let huge = Layout::from_size_align(isize::MAX as usize - 4096, 8).unwrap();
        assert!(alloc.allocate(huge).is_err());
        assert_eq!(alloc.metrics(), Metrics::default());
    }
}
