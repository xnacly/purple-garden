use std::sync::atomic::AtomicI64;

use purple_garden::{GardenOpaque, pg_pkg};

#[derive(GardenOpaque, Debug)]
/// A thread safe counter.
pub struct Counter {
    value: AtomicI64,
}

/// The Purple Garden package exported by this example.
///
/// The macro expands this module into VM wrappers and package metadata. The
/// build script reads that metadata and writes `extern.garden` for tooling and
/// editor integration.
///
/// Meaning for instance: the lsp will show completions for methods in the
/// counter package, show signatures on hover and diagnostics as if the package were
/// defined in the interpreter.
#[pg_pkg]
pub mod counter {
    use super::Counter;
    use std::sync::atomic::{AtomicI64, Ordering};

    /// Create a new counter from an initial value.
    pub fn new(value: i64) -> Counter {
        Counter {
            value: AtomicI64::new(value),
        }
    }

    /// Increment the counter and return the updated value.
    pub fn increment(counter: &Counter) -> i64 {
        counter.value.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Read the current counter value.
    pub fn get(counter: &Counter) -> i64 {
        counter.value.load(Ordering::SeqCst)
    }
}
