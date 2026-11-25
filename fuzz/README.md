# Fuzzing

[cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets, in a workspace of their own.

| target    | input                     | checks                                                              |
| --------- | ------------------------- | ------------------------------------------------------------------- |
| `compile` | raw bytes                 | the compiler at every `-O`, with and without the JIT, never panics  |
| `diff`    | a generated program       | the VM at `-O1..3` and the JIT at `-O0..3` agree with the VM at `-O0` |

The generator (`src/generate.rs`) only writes well-typed programs that terminate: functions call
only themselves or earlier ones and pass a decreasing `fuel`. Each ends in an `Int` checksum over
everything it bound, so backends compare one value, or the anomaly that stopped them.

```sh
cargo install cargo-fuzz
cargo fuzz run -O compile fuzz/corpus/compile examples -- -dict=fuzz/garden.dict -timeout=20
# the JIT only compiles on x86-64; on Apple silicon add --target x86_64-apple-darwin (Rosetta)
cargo fuzz run -O diff -- -timeout=20 -len_control=0 -max_len=4096

cargo fuzz fmt diff fuzz/artifacts/diff/<file>     # print a failing program (on stderr)
cargo fuzz tmin diff fuzz/artifacts/diff/<file>    # minimise it
```

`-len_control=0 -max_len=4096` gives the generator enough input from the start; libFuzzer's
default grows inputs a few bytes at a time, which only makes trivial programs for minutes.
`cargo fuzz` builds with debug assertions, so the VM's checks of bytecode invariants run too.
