# Cross-language benchmarks

purple-garden on wall, cpu, peak rss against: 

- CPython (`python3`)
- JavaScriptCore (`bun`)
- LuaJIT (`luajit`)

## Running

```sh
cargo build --release -p purple-garden-cli
cargo run --release -p purple-garden-bench -- run --out results.json
```

Requires `hyperfine`, `bun`, `luajit` and `python3` in path;
a runtime that is not installed is skipped (`--strict` makes it an error).

A workload directory holds `main.garden` and optionally `main.js`, `main.lua`
and `main.py`. A runtime without a port is skipped for that workload, so a
port that would take minutes (CPython on `nqueens`) can simply be left out.
