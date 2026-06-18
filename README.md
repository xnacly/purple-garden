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
- Embeddable with 0 friction for Rust interop, see
  [help/embed](./help/embed.md) for a guide and
  [examples/embed-counter/](./examples/embed-counter/) for its real world
  counter part.
- Memory efficient, with an optional garbage collector and a minimal standard
  library, see [std](./purple-garden-std/src)
- Editor support via language server protocol implementation and tree-sitter
  grammar, see [tree-sitter/README.md](./tree-sitter/README.md) (VS Code extension
  is a work in progress)

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
cargo bench
```

For a fast run with less statistical confidence:

```bash
PG_BENCH_QUICK=1 cargo bench
```

`PG_BENCH_QUICK=1` uses 10 samples, a 100ms warm-up, and a 300ms measurement
window per benchmark.
