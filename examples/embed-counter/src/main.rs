use std::sync::atomic::AtomicI64;

use purple_garden::{FromVm, IntoVm, Pg, PgType, pg_pkg};

#[derive(PgType, FromVm, IntoVm, Debug)]
/// A thread safe counter
pub struct Counter {
    value: AtomicI64,
}

/// The purple garden wrapper for Counter, usable in purple-garden like so:
///
/// ```garden
/// import ("counter" "testing")
///
/// let c = counter.new(0)
/// counter.increment(c)
/// counter.increment(c)
/// counter.increment(c)
/// testing.assert(counter.get(c) == 3)
/// ```
#[pg_pkg]
pub mod counter {
    use super::Counter;
    use std::sync::atomic::{AtomicI64, Ordering};

    /// Creates a new counter.
    pub fn new(value: i64) -> Counter {
        Counter {
            value: AtomicI64::new(value),
        }
    }

    /// Increments counter and returns the new value.
    pub fn increment(counter: &Counter) -> i64 {
        counter.value.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Returns the current counter value.
    pub fn get(counter: &Counter) -> i64 {
        counter.value.load(Ordering::SeqCst)
    }
}

fn main() {
    let input = include_bytes!("counter.garden");
    let mut program = Pg::new()
        .with_stdlib()
        .with_lib(&counter::PACKAGE)
        .compile(input)
        .expect("counter script should compile");

    let counter = program
        .run_take::<&Counter>()
        .expect("counter script should run");

    counter::increment(counter);
    assert_eq!(counter::get(counter), 4);
    println!("{:#?}", counter);
}
