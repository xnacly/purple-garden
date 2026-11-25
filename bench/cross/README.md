# Cross-language benchmarks

purple-garden on wall, cpu, peak rss against: 

- JavaScriptCore (`bun`)
- LuaJIT (`luajit`)

Every runtime with a JIT also runs as `<name>-nojit` with the JIT disabled
(`--no-jit`, `BUN_JSC_useJIT=0`, `luajit -joff`), to compare interpreters.

## Running

```sh
cargo build --release -p purple-garden-cli
cargo run --release -p purple-garden-bench -- run --out results.json
```

Requires `hyperfine`, `bun`, and `luajit` in path;
a runtime that is not installed is skipped (`--strict` makes it an error).

A workload directory holds `main.garden` and optionally `main.js` and
`main.lua`. A runtime without a port is skipped for that workload.
