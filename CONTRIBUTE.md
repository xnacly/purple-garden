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
