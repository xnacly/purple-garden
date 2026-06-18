# Embedding Purple Garden

Purple Garden packages can be exposed from ordinary Rust functions. Most users
should write normal Rust types and annotate a module; the macro builds the VM
wrappers and package metadata.

## Defining A Foreign Type

Use the derives for Rust types that should cross the VM boundary as opaque
foreign handles:

```rust
use std::sync::atomic::{AtomicI64, Ordering};
use purple_garden::{FromVm, IntoVm, PgType};

#[derive(PgType, FromVm, IntoVm)]
pub struct Counter {
    value: AtomicI64,
}

impl Counter {
    fn new(value: i64) -> Self {
        Self { value: AtomicI64::new(value) }
    }

    fn increment(&self) -> i64 {
        self.value.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn get(&self) -> i64 {
        self.value.load(Ordering::SeqCst)
    }
}
```

The derives map `Counter` to `Foreign<Counter>`. The VM stores an opaque
handle; Purple Garden code cannot inspect the fields directly.

## Defining A Package

Write normal Rust functions inside a `#[pg_pkg]` module:

```rust
use purple_garden::{pg_fn, pg_pkg};

#[pg_pkg]
pub mod counter {
    use super::Counter;

    /// Creates a new counter.
    pub fn new(value: i64) -> Counter {
        Counter::new(value)
    }

    /// Increments counter and returns the new value.
    pub fn increment(counter: &Counter) -> i64 {
        counter.increment()
    }

    /// Returns the current counter value.
    #[pg_fn(pure)]
    pub fn get(counter: &Counter) -> i64 {
        counter.get()
    }
}
```

The macro generates `counter::PACKAGE`, VM wrappers, docs, argument names, type
metadata, and the `pure` flag.

## Tooling Signatures

If you want to ship package signatures for tooling, generated package metadata
can be rendered into an `extern.garden` file during a build step. The example
crate writes `extern.garden` next to the crate so editors and LSP tooling can
load it without a custom runtime flag.

The runtime API also exposes `Pkg::extern_source()` for callers that want to
generate the file themselves.

For instance the above generates:

```garden
#! The Purple Garden package exported by this example.
#!
#! The macro expands this module into VM wrappers and package metadata. The
#! build script reads that metadata and writes `extern.garden` for tooling and
#! editor integration.
#!
#! Meaning for instance: the LSP will show completions for methods in the
#! counter package, show signatures on hover and diagnostics as if the package
#! were defined in the interpreter.
extern "counter" {
    #! Create a new counter from an initial value.
    fn new(value: Int) Foreign<Counter>
    #! Increment the counter and return the updated value.
    fn increment(counter: Foreign<Counter>) Int
    #! Read the current counter value.
    fn get(counter: Foreign<Counter>) Int
}
```

## Calling From Purple Garden

Once registered with the embedding API, the package is used like any other
package:

```garden
import ("counter" "testing")

let c = counter.new(0)
counter.increment(c)
counter.increment(c)
counter.increment(c)
c
```

Embedding code can compile that script with the package:

```rust
let mut program = purple_garden::Pg::new()
    .with_stdlib()
    .with_lib(&counter::PACKAGE)
    .compile(include_bytes!("counter.garden"))?;
let counter: &Counter = program.run_take()?;
```

The complete runnable example lives in `examples/embed-counter`.

Run it with:

```sh
cargo run -p purple-garden-embed-counter
```
