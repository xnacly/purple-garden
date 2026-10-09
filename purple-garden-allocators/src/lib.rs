#![feature(allocator_api)]
//! Crate using the new allocator trait, see
//! [trait::Allocator](https://doc.rust-lang.org/std/alloc/trait.Allocator.html) to create and
//! interact with a list of allocators, ranging from low level allocators like the PageAlloc, to the
//! ArenaAlloc, the StackAlloc and the MetricAlloc.

pub mod page;
