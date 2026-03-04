# purple_garden

purple_garden is a lean scripting language designed for performance, with a
focus on aggressive compile-time optimisations, JIT compilation, fine-grained
memory control, and optional garbage collection.

```python
import ("io")
io.println("Hello World")
```

## Features / Design Goals

- Extremely fast execution with a register-based VM and aggressive compile-time
  optimisations (both IR and peephole), see the [ir](./src/ir/) and
  [opt](./src/opt) modules
- JIT compilation for runtime hotspots
- Embeddable with minimal friction for Rust interop
- Memory efficient, with an optional garbage collector and a minimal standard
  library

## Embedding

> THIS IS A BIG WIP

```rust
let pg = purple_garden::init(purple_garden::Config{});
pg.register_function("__todo", |_, args| {
    if args.is_empty() {
        return None;
    }
    panic!("{}", args[0]) // panics on call with Display of purple_garden::vm::Value
    None
});
let return_value = pg.run(`__todo("hello world")`.as_bytes()); // None
```

## Local Setup

> Nightly Rust is required due to branch prediction optimisations in the VM.

```bash
git clone git@github.com:xnacly/purple-garden.git
cargo +nightly run -- --help
```

### Benchmarks

> These may take a while

```bash
cargo bench --features nightly
```
