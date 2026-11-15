# Contributing

## Project Structure

Purple Garden is a Rust workspace split by responsibilities:

- `purple-garden/`: public library crate, integration tests, and benchmarks.
- `purple-garden-frontend/`: lexing, parsing, type checking, and lowering into IR.
- `purple-garden-ir/`: shared IR data structures, types, display, and analysis helpers.
- `purple-garden-opt/`: IR and bytecode optimization passes.
- `purple-garden-bc/`: bytecode lowering and bytecode register allocation.
- `purple-garden-jit/`: native code generation backends and JIT register allocation.
- `purple-garden-runtime/`: bytecode VM, runtime values, allocation, and VM ops.
- `purple-garden-std/`: standard library package resolution.
- `purple-garden-cli/`: command-line entry point.
- `purple-garden-shared/`: shared configuration, tracing, mmap, and cross-crate utilities.
- `purple-garden-macros/`: procedural macros used by the project.

## Commit Conventions

Use small, atomic commits. A commit should introduce one behavior change, one
test addition, one refactor, or one documentation update.

Commit subjects use the affected area as a prefix:

- `frontend: ...`
- `ir: ...`
- `opt: ...`
- `bc: ...`
- `jit: ...`
- `runtime: ...`
- `docs: ...`

Keep commit subjects imperative and specific. When adding a backend lowering or
optimization, prefer committing the tests first when they are useful as a review
boundary.

## Source Conventions

- Prefer existing local helpers and pass structure over new abstractions.
- Keep compiler transformations conservative unless the invariants are explicit
- Use `purple_garden_shared::trace!` for optimization rewrites. Optimization traces should include the pass name
- Do not add a duplicate defensive checks when invariants are already validated
- Prefer focused unit tests around pass contracts and backend instruction
  lowering, plus integration tests for user-visible behavior.

## Finding Memory Bugs

The runtime, the allocators and the JIT use `unsafe` heavily. Three tools check
it beyond the regular test suite; none of them runs in CI.

### Miri

```sh
rustup +nightly component add miri
MIRIFLAGS=-Zmiri-disable-isolation cargo miri test -p purple-garden-runtime
```

Miri interprets the tests and reports undefined behaviour: aliasing violations,
use after free, out of bounds and misaligned accesses. Under `cfg(miri)` the
page allocator hands out its pages from the global allocator, because Miri
neither knows `getpagesize` nor models a partial `munmap`, so every crate that
allocates through `PageAlloc` runs.

- Run it per crate; it is slow (minutes to tens of minutes per crate).
- Add `-Zmiri-ignore-leaks` for tests that leak arenas on purpose.
- It cannot run native JIT code, inline assembly or the CLI tests. On x86-64
  the runtime's assembly trampolines are tested too; skip them with
  `cargo miri test -p purple-garden-runtime -- --skip jit_helpers`.
- The GC turns integers into pointers, so Miri may miss bugs through GC
  object pointers; the GC stress mode below covers those.

### AddressSanitizer

```sh
RUSTFLAGS=-Zsanitizer=address cargo test --workspace --target aarch64-apple-darwin
```

Pass the host target explicitly (`x86_64-unknown-linux-gnu`,
`x86_64-apple-darwin`, ...), otherwise build scripts and proc macros get
instrumented too. Use an x86-64 target to cover the JIT. ASan sees heap
memory from the Rust allocator, but not GC objects, which live in pages the
GC maps itself.

### GC stress

```sh
cargo test --workspace --features purple-garden-runtime/gc_stress
cargo run --features gc_stress -- examples/arrays.garden
```

The `gc_stress` feature runs the collect hook before every allocation and
places every GC object in a mapping of its own, its payload flush against a
`PROT_NONE` guard page. A read or write past an object faults at once instead
of corrupting its neighbour. It covers the bytecode VM, the builtins and
jitted code alike, and turns every allocation into a collection point for the
day `collect` frees memory.

## Compiler Pipeline

The broad pipeline is:

1. Source text is tokenized, parsed, and type checked in `purple-garden-frontend`.
2. The frontend lowers typed syntax into `purple-garden-ir` functions.
3. `purple-garden-opt` applies IR optimization passes.
4. The optimized IR is lowered either to bytecode in `purple-garden-bc` or native
   code in `purple-garden-jit` when the JIT accepts the function (Jit makes the
    decision based on supported IR concepts, used register count and other
    factors).
5. The runtime executes bytecode and dispatches native/JIT functions through the
   shared runtime representation.

## Optimization Passes

IR optimization are in `purple-garden-opt/src/ir/`. Bytecode peephole
passes in `purple-garden-opt/src/bc/`.

When adding or changing an optimization pass:

- Put the pass in its own module.
- Re-export it from `purple-garden-opt/src/ir/mod.rs` or the matching `bc` module.
- Wire it into the pass pipeline in `purple-garden-opt/src/lib.rs`.
- Add focused tests in the pass module.
- Add or update optimization trace calls for each observable rewrite.
- Update [docs/opt.md](doc/opt.md) with the pass purpose, scope, and ordering.
