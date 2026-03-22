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
- JIT compilation for runtime hotspots, or for everything with `--native`
- Embeddable with minimal friction for Rust interop via `vm::BuiltinFn`
- Memory efficient, with an optional garbage collector and a minimal standard
  library, see [std](./src/std/)

## Local Setup

> Nightly Rust is required due to branch prediction optimisations in the VM.

```bash
git clone git@github.com:xnacly/purple-garden.git
cargo +nightly run -- --help
```

### Benchmarks

> These may take a while

```bash
cargo +nightly bench --features nightly
```
