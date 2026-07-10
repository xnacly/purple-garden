# Embedding Purple Garden

Purple Garden packages can be exposed from ordinary Rust functions. Most users
should write normal Rust types and annotate a module; the macro builds the VM
wrappers and package metadata.

## Defining A Foreign Type

Use the derives for Rust types that should cross the VM boundary as opaque
foreign handles:

```rust
use std::sync::atomic::{AtomicI64, Ordering};
use purple_garden::GardenOpaque;

#[derive(GardenOpaque)]
pub struct Counter {
    value: AtomicI64,
}
```

The derive maps `Counter` to `Foreign<Counter>`. The VM stores an opaque
handle; Purple Garden code cannot inspect the fields directly.

Use `GardenOpaque` for stateful objects, resources, handles, or any type whose
Rust representation should stay private or cant be represented in purple
garden. Package functions can return the owned type and can accept `&Counter`
or `&mut Counter` to operate on the same handle.

## Defining A Garden Value

Use `GardenValue` for Rust types that can be represented as purple garden
values. These are values directly instantiatable from a garden script, can be
passed around, and inspected according to their purple garden type. The current
derive supports named-field Rust structs, which become anonymous records:

```rust
use purple_garden::GardenValue;

#[derive(GardenValue)]
pub struct User {
    name: String,
    age: i64,
}

#[derive(GardenValue)]
pub struct Account {
    owner: User,
    active: bool,
}
```

The derive maps `User` to `Record<name: Str age: Int>` and `Account` to
`Record<owner: Record<name: Str age: Int> active: Bool>`. Records are just
purple garden values: package functions can accept them, return them, and
expose their field types to diagnostics, completions, and generated
`extern.garden` files.

Deriving `GardenValue` implements these embedding traits for the Rust type:

- `PgType`, so package metadata can describe the Garden type.
- `FromVm`, so wrappers can decode a VM value into the Rust representation.
- `IntoVm`, so wrappers can encode a Rust value back into the VM.

Every field type must itself implement those traits. Built-in Rust mappings
include `String`, `&str`, `i64`, `f64`, and `bool`; nested `GardenValue` structs
work as record fields.

## Defining A Package

Create a rust module annotated with `#[pg_pkg]`, this automatically creates the
meta data, the VM wrappers, the docs, the purple garden value <-> rust value
conversion:

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

> The `#[pg_fn(pure)]` enables the purple garden optimiser to constant fold the function call if its arguments are constant
> / known at compile time

The generated wrapper uses the embedding traits at the rust/purple garden
function boundary:

- Each Rust argument type must implement `FromVm`. When Garden calls the
  package function, the wrapper reads the raw VM argument value and decodes it
  into the Rust argument before calling your function.
- A non-`()` Rust return type must implement `IntoVm`. After your function
  returns, the wrapper encodes the Rust value into a VM value and writes it to
  the return register.
- Each argument and return type must also implement `PgType`, so package
  metadata and generated `extern.garden` signatures can describe the function.

For ordinary embedded types, derive `GardenValue` or `GardenOpaque` instead of
implementing these traits by hand. The `examples/embed-config` example below
shows a complete package that accepts nested Garden record values directly.

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
    .with_lib(&counter::PACKAGE)
    .compile(include_bytes!("counter.garden"))?;
let counter: &Counter = program.run_take()?;
```

## Full examples

### Embed-counter

The complete runnable counter example lives in `examples/embed-counter`.

Run it with:

```sh
cargo run -p purple-garden-embed-counter
```

### Embed-config

There is also a record-value example in `examples/embed-config`. It defines
nested `GardenValue` structs in Rust, accepts them from purple-garden, and
returns a Rust-generated summary string:

```garden
import ("config")

config.summary({
    service: "api"
    workers: 4
    debug: true
    retry: {
        attempts: 3
        backoff_ms: 250
    }
})
```

Run it with:

```sh
cargo run -p purple-garden-embed-config
```
