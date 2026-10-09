#![feature(allocator_api, likely_unlikely)]
#![cfg_attr(test, feature(vec_push_within_capacity))]
//! Crate using the new allocator trait, see
//! [trait::Allocator](https://doc.rust-lang.org/std/alloc/trait.Allocator.html) to create and
//! interact with a list of allocators, ranging from low level allocators like the PageAlloc, to the
//! ArenaAlloc, the StackAlloc and the MetricAlloc.

pub mod bump;
pub mod metric;
pub mod page;
pub mod stack;
