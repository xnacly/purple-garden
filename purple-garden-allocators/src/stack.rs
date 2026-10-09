//! LIFO allocations out of a borrowed buffer, see [`StackAlloc`].

use std::alloc::{AllocError, Allocator, Layout};
use std::cell::Cell;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ptr::NonNull;

/// [`Allocator`] sizes round up to a multiple of this, so `top` always is one.
///
/// Without it, mixed alignments leave padding behind: 3 bytes at offset 0,
/// then 8 bytes aligned to 8 at offset 8. Freeing the 8 bytes moves `top` back
/// to 8, but the 3 bytes end at 3, so they never count as the top allocation
/// again and stay lost until [`StackAlloc::reset`]. Rounded, every allocation
/// aligned up to 16 starts right at `top` and frees in reverse always match.
///
/// [`StackAlloc::push`] and [`StackAlloc::pop`] don't round, they take exact
/// sizes for one element type at a time.
pub const UPROUND: usize = 16;

#[inline(always)]
fn granules(size: usize) -> usize {
    size.next_multiple_of(UPROUND)
}

/// Hands out memory from a buffer it borrows, typically one on the machine
/// stack, by bumping `top` and takes it back in reverse order.
///
/// Allocations round up to [`UPROUND`]. Freeing anything but the most recent
/// allocation is ignored, its memory comes back once everything above it is
/// freed.
#[derive(Debug)]
pub struct StackAlloc<'buf> {
    block: NonNull<u8>,
    cap: usize,
    /// Bytes in use, the next allocation starts here.
    top: Cell<usize>,
    _buf: PhantomData<&'buf mut [MaybeUninit<u8>]>,
}

// It holds the only borrow of its buffer, so it can move to another thread.
unsafe impl Send for StackAlloc<'_> {}

impl<'buf> StackAlloc<'buf> {
    /// Allocate out of `buf`, starting at its first [`UPROUND`] aligned byte.
    pub fn new(buf: &'buf mut [MaybeUninit<u8>]) -> Self {
        let skip = buf.as_ptr().align_offset(UPROUND).min(buf.len());
        let block = NonNull::from(&mut buf[skip..]).cast::<u8>();
        Self {
            block,
            cap: (buf.len() - skip) / UPROUND * UPROUND,
            top: Cell::new(0),
            _buf: PhantomData,
        }
    }

    /// Bytes in use.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.top.get()
    }

    /// Free everything at once. Takes `&mut self`, so nothing allocated from
    /// the stack is still borrowed.
    pub fn reset(&mut self) {
        self.top.set(0);
    }

    /// Push `value`, `false` if it doesn't fit. Pushes take exactly
    /// `size_of::<T>()`, so `top` stays aligned while everything pushed shares
    /// one alignment; don't interleave them with [`Allocator`] allocations.
    #[inline(always)]
    pub fn push<T: Copy>(&self, value: T) -> bool {
        let top = self.top.get();
        debug_assert!(top.is_multiple_of(align_of::<T>()));
        let end = top + size_of::<T>();
        if std::hint::unlikely(end > self.cap) {
            return false;
        }
        unsafe { self.block.add(top).cast::<T>().write(value) };
        self.top.set(end);
        true
    }

    /// Pop the value [`StackAlloc::push`] pushed last.
    ///
    /// # Safety
    ///
    /// The top `size_of::<T>()` bytes hold a `T` pushed with `push`.
    #[inline(always)]
    pub unsafe fn pop<T: Copy>(&self) -> T {
        let top = self.top.get() - size_of::<T>();
        self.top.set(top);
        unsafe { self.block.add(top).cast::<T>().read() }
    }

    /// Push `values` with one capacity check, each written on its own: a
    /// differently split read of a wide write stalls on store forwarding.
    #[inline(always)]
    pub fn push_all<T: Copy, const N: usize>(&self, values: [T; N]) -> bool {
        let top = self.top.get();
        debug_assert!(top.is_multiple_of(align_of::<T>()));
        let end = top + N * size_of::<T>();
        if std::hint::unlikely(end > self.cap) {
            return false;
        }
        let dst = unsafe { self.block.add(top).cast::<T>() };
        for (i, value) in values.into_iter().enumerate() {
            unsafe { dst.add(i).write(value) };
        }
        self.top.set(end);
        true
    }

    /// Pop what [`StackAlloc::push_all`] pushed last, in push order.
    ///
    /// # Safety
    ///
    /// The top `N` elements hold `T`s pushed with `push` or `push_all`.
    #[inline(always)]
    pub unsafe fn pop_all<T: Copy, const N: usize>(&self) -> [T; N] {
        let top = self.top.get() - N * size_of::<T>();
        self.top.set(top);
        let src = unsafe { self.block.add(top).cast::<T>() };
        std::array::from_fn(|i| unsafe { src.add(i).read() })
    }

    fn offset(&self, ptr: NonNull<u8>) -> usize {
        ptr.as_ptr() as usize - self.block.as_ptr() as usize
    }

    fn is_top(&self, ptr: NonNull<u8>, layout: Layout) -> bool {
        self.offset(ptr) + granules(layout.size()) == self.top.get()
    }
}

/// `&StackAlloc` comes with it, through core's `impl Allocator for &A`, for
/// collections sharing one stack; `Vec<T, StackAlloc>` owns its own.
unsafe impl Allocator for StackAlloc<'_> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let base = self.block.as_ptr() as usize;
        let start = (base + self.top.get()).next_multiple_of(layout.align()) - base;
        let end = start
            .checked_add(granules(layout.size()))
            .filter(|&end| end <= self.cap)
            .ok_or(AllocError)?;
        self.top.set(end);
        let ptr = unsafe { self.block.add(start) };
        Ok(NonNull::slice_from_raw_parts(ptr, layout.size()))
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if self.is_top(ptr, layout) {
            self.top.set(self.offset(ptr));
        }
    }

    /// The topmost allocation grows in place, anything else moves to the top.
    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        if self.is_top(ptr, old) && (ptr.as_ptr() as usize).is_multiple_of(new.align()) {
            let end = self
                .offset(ptr)
                .checked_add(granules(new.size()))
                .filter(|&end| end <= self.cap)
                .ok_or(AllocError)?;
            self.top.set(end);
            return Ok(NonNull::slice_from_raw_parts(ptr, new.size()));
        }
        let moved = self.allocate(new)?;
        unsafe {
            ptr.copy_to_nonoverlapping(moved.cast(), old.size());
            self.deallocate(ptr, old);
        }
        Ok(moved)
    }

    /// Memory freed earlier is reused as is, so the grown part is zeroed here.
    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        let block = unsafe { self.grow(ptr, old, new)? };
        unsafe {
            block
                .cast::<u8>()
                .add(old.size())
                .write_bytes(0, new.size() - old.size());
        }
        Ok(block)
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        if !(ptr.as_ptr() as usize).is_multiple_of(new.align()) {
            let moved = self.allocate(new)?;
            unsafe {
                ptr.copy_to_nonoverlapping(moved.cast(), new.size());
                self.deallocate(ptr, old);
            }
            return Ok(moved);
        }
        if self.is_top(ptr, old) {
            self.top.set(self.offset(ptr) + granules(new.size()));
        }
        Ok(NonNull::slice_from_raw_parts(ptr, new.size()))
    }
}

#[cfg(test)]
mod tests {
    use super::StackAlloc;
    use std::alloc::{Allocator, Layout};
    use std::mem::MaybeUninit;

    fn buffer<const N: usize>() -> [MaybeUninit<u8>; N] {
        [MaybeUninit::uninit(); N]
    }

    fn layout(size: usize, align: usize) -> Layout {
        Layout::from_size_align(size, align).unwrap()
    }

    #[test]
    fn allocations_bump_aligned_and_free_in_reverse() {
        let mut buf = buffer::<4096>();
        let stack = StackAlloc::new(&mut buf);
        let a = stack.allocate(layout(3, 1)).unwrap();
        let b = stack.allocate(layout(8, 8)).unwrap();
        assert_eq!(b.cast::<u8>().as_ptr() as usize % 8, 0);
        assert_eq!(stack.depth(), 32);
        unsafe { stack.deallocate(b.cast(), layout(8, 8)) };
        assert_eq!(stack.depth(), 16);
        unsafe { stack.deallocate(a.cast(), layout(3, 1)) };
        assert_eq!(stack.depth(), 0);
    }

    #[test]
    fn out_of_order_frees_wait_for_the_blocks_above() {
        let mut buf = buffer::<4096>();
        let stack = StackAlloc::new(&mut buf);
        let a = stack.allocate(layout(16, 8)).unwrap();
        let b = stack.allocate(layout(16, 8)).unwrap();
        unsafe { stack.deallocate(a.cast(), layout(16, 8)) };
        assert_eq!(stack.depth(), 32);
        unsafe { stack.deallocate(b.cast(), layout(16, 8)) };
        assert_eq!(stack.depth(), 16);
    }

    #[test]
    fn fails_past_capacity() {
        let mut buf = buffer::<4096>();
        let stack = StackAlloc::new(&mut buf);
        assert!(stack.allocate(layout(stack.cap, 1)).is_ok());
        assert!(stack.allocate(layout(1, 1)).is_err());
    }

    #[test]
    fn the_top_allocation_grows_in_place() {
        let mut buf = buffer::<{ 1 << 16 }>();
        let stack = StackAlloc::new(&mut buf);
        let mut values = Vec::with_capacity_in(4, &stack);
        values.extend(0..4u64);
        let before = values.as_ptr();
        values.extend(4..1000);
        assert_eq!(values.as_ptr(), before);
        assert_eq!(stack.depth(), (values.capacity() * 8).next_multiple_of(16));
        assert_eq!(values.iter().sum::<u64>(), 999 * 1000 / 2);
    }

    #[test]
    fn grown_zeroed_memory_is_zero_after_reuse() {
        let mut buf = buffer::<4096>();
        let stack = StackAlloc::new(&mut buf);
        let dirty = stack.allocate(layout(64, 8)).unwrap();
        unsafe {
            dirty.cast::<u8>().write_bytes(0xff, 64);
            stack.deallocate(dirty.cast(), layout(64, 8));
        }
        let small = stack.allocate(layout(8, 8)).unwrap();
        let grown =
            unsafe { stack.grow_zeroed(small.cast(), layout(8, 8), layout(64, 8)) }.unwrap();
        assert!(unsafe { grown.as_ref() }[8..].iter().all(|&b| b == 0));
    }

    #[test]
    fn uses_the_aligned_part_of_any_buffer() {
        let mut buf = buffer::<1000>();
        let unaligned = &mut buf[1..];
        let start = unaligned.as_ptr() as usize;
        let stack = StackAlloc::new(unaligned);
        let block = stack.allocate(layout(8, 8)).unwrap();
        let at = block.cast::<u8>().as_ptr() as usize;
        assert_eq!(at % 16, 0);
        assert!(at >= start && at - start < 16);
        assert!(stack.allocate(layout(1000, 1)).is_err());
    }

    #[test]
    fn collections_can_own_their_stack() {
        let mut buf = buffer::<4096>();
        let mut frames = Vec::with_capacity_in(4, StackAlloc::new(&mut buf));
        frames.extend(0..4u64);
        assert!(frames.push_within_capacity(4).is_err());
        assert_eq!(frames.pop(), Some(3));
        assert_eq!(frames.allocator().depth(), 32);
    }

    #[test]
    fn push_and_pop_are_lifo_until_full() {
        let mut buf = buffer::<4096>();
        let mut stack = StackAlloc::new(&mut buf);
        assert!(stack.push(1u64));
        assert!(stack.push_all([2u64, 3]));
        assert_eq!(unsafe { stack.pop_all::<u64, 2>() }, [2, 3]);
        assert_eq!(unsafe { stack.pop::<u64>() }, 1);
        while stack.push(0u64) {}
        assert_eq!(stack.depth(), 4096);
        stack.reset();
        assert_eq!(stack.depth(), 0);
    }
}
