# Fuzzing

cargo-fuzz targets, in a workspace of their own (the main workspace excludes `fuzz/`, so
`cargo build --workspace` needs neither libFuzzer nor nightly sanitizers).

| target    | input                       | checks                                                                                   |
| --------- | --------------------------- | ---------------------------------------------------------------------------------------- |
| `parse`   | raw bytes                   | lexer and parser return an AST or diagnostics, never panic                               |
| `compile` | raw bytes                   | the whole compiler with the safe stdlib, every `-O` with and without the JIT, never panics |
| `diff`    | bytes driving the generator | well-typed, terminating programs agree on every backend, see `purple-garden-fuzz`         |

`cargo fuzz` builds with debug assertions, which make the optimiser and the lowering verify the
IR after every pass (`purple_garden_ir::Func::verify`).

```sh
# raw input targets, seeded from the examples
cargo fuzz run -O parse fuzz/corpus/parse examples -- -dict=fuzz/garden.dict
cargo fuzz run -O compile fuzz/corpus/compile examples -- -dict=fuzz/garden.dict -timeout=20

# differential: the JIT only compiles on x86-64, on Apple silicon through Rosetta
cargo fuzz run -O diff --target x86_64-apple-darwin fuzz/corpus/diff -- -timeout=20
```

`--features gc_stress` puts every object the programs allocate behind a guard page (see
purple-garden-runtime's `gc_stress`), so an access past an object faults where it happens. Use
it for `diff` runs; the seeded test takes it too (`cargo test -p purple-garden-fuzz --features
gc_stress`).

Long unattended runs: add `-max_total_time=<s> -fork=<jobs> -ignore_crashes=1` and look at
`fuzz/artifacts/<target>/` afterwards.

## Without libFuzzer

The generator and the oracle live in `purple-garden-fuzz`, so the differential check also runs
as a plain test over seeds, and `pg-diff` explores, classifies and minimises:

```sh
PG_DIFF_SEEDS=20000 cargo test -p purple-garden-fuzz --release --target x86_64-apple-darwin
cargo run -p purple-garden-fuzz --bin pg-diff -- run 0 10000      # failing seeds, classified
cargo run -p purple-garden-fuzz --bin pg-diff -- show 42 > p.garden
cargo run -p purple-garden-fuzz --bin pg-diff -- check p.garden   # every backend's outcome
cargo run -p purple-garden-fuzz --bin pg-diff -- reduce p.garden min.garden
```

`reduce` runs every candidate in a child process, so it minimises crashes too. With
`PG_REDUCE_ANY_BACKEND=1` any backend may diverge, for nondeterministic failures.
