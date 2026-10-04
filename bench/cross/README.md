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
