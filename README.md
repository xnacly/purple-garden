# purple_garden

purple_garden is a lean scripting language designed for performance, with a
focus on aggressive compile-time optimisations, JIT compilation, fine-grained
memory control, and optional garbage collection. It is built to be easily
embedded and extended from Rust.

```garden
import ("io")
io.println("Hello World")
```

## Features / Design Goals

- Extremely fast execution with a register-based VM, aggressive compile-time
  optimisations enabled by SSA IR and hardware near bytecode design, see the
  [ir](./purple-garden-opt/src/ir) and
  [opt](./purple-garden-opt/src/bc) modules
- JIT compilation for the whole input by default (can be disabled with `--no-jit`)
- Embeddable with 0 friction for Rust interop; see the example below and
  [help/embed](./help/embed.md) for the complete API.
- Memory efficient, with an optional garbage collector and a minimal standard
  library, see [std](./purple-garden-std/src)
- Editor support via language server protocol implementation and tree-sitter
  grammar, see [tree-sitter/README.md](./tree-sitter/README.md) (VS Code extension
  is a work in progress)

## Embedding

```rust
use std::{collections::HashMap, sync::Mutex};
use purple_garden::{pg_pkg, GardenOpaque, GardenValue, Pg};

#[derive(GardenOpaque)]
struct Store(Mutex<HashMap<String, Profile>>);

#[derive(GardenValue, Clone)]
struct Profile { name: String, plan: String }

#[pg_pkg]
mod store {
    use super::{Profile, Store};

    pub fn put(store: &Store, key: String, value: Profile) {
        store.0.lock().unwrap().insert(key, value);
    }
    pub fn get(store: &Store, key: String, fallback: Profile) -> Profile {
        store.0.lock().unwrap().get(&key).cloned().unwrap_or(fallback)
    }
}

// parse and compile a program from source code with above defined store module
let mut program = Pg::new().with_lib(&store::PACKAGE).compile(br#"
    import "store"
    fn provision(cache: Foreign<Store>) Str {
        store.put(cache "user:42" { name: "Ada" plan: "pro" })
        let profile = store.get(cache "user:42" { name: "n/a" plan: "free" })
        profile.name
    }
"#)?;

let cache = Store(Mutex::new(HashMap::new()));
// locate "provision" function by name and validate types
let provision = program.function::<(&Store,), String>("provision").unwrap();
// call function with a reference to the cache / store
let output = program.call(&provision, (&cache,))?;

assert_eq!(output, "Ada");
assert_eq!(cache.0.lock().unwrap()["user:42"].plan, "pro");
# Ok::<(), Box<dyn std::error::Error>>(())
```

For in depth docs consult the [help/embed](./help/embed.md) guide.

## Documentation

For an intro to purple garden see
[help/intro.md](./help/intro.md) or run
`purple-garden intro` after installing the binary.

## Local Setup

> Nightly Rust is required due to:
>
> - branch prediction hints in the vm
> - simd in the lexer

```bash
git clone git@github.com:xnacly/purple-garden.git
cargo run -- --help
```

### Benchmarks

> These may take a while

```bash
set -x RUSTFLAGS "-C target-cpu=native"
cargo bench --workspace
```

For a fast run with less statistical confidence:

```bash
set -x RUSTFLAGS "-C target-cpu=native"
PG_BENCH_QUICK=1 cargo bench --workspace
```

`PG_BENCH_QUICK=1` uses 10 samples, a 100ms warm-up, and a 300ms measurement
window per benchmark.
