```text
         ,            ,            ,    
     /\^/`\       /\^/`\       /\^/`\   
    | \/   |     | \/   |     | \/   |  
    | |    |     | |    |     | |    |  
    \ \    /     \ \    /     \ \    /  
     '\\//'       '\\//'       '\\//'   
       ||           ||           ||     
       ||           ||           ||     
       ||           ||           ||     
       ||  ,        ||  ,        ||  ,  
   |\  ||  |\   |\  ||  |\   |\  ||  |\ 
   | | ||  | |  | | ||  | |  | | ||  | |
   | | || / /   | | || / /   | | || / / 
    \ \||/ /     \ \||/ /     \ \||/ /  
     `\\//`       `\\//`       `\\//`   
    ^^^^^^^^     ^^^^^^^^     ^^^^^^^^  
```

# purple_garden

purple_garden is a lean scripting language designed for performance, with a
focus on aggressive compile-time optimisations, JIT compilation, fine-grained
memory control, and optional garbage collection.

```garden
import ("io")
io.println("Hello World")
```

## Features / Design Goals

- Extremely fast execution with a register-based VM and aggressive compile-time
  optimisations (both IR and peephole), see the
  [ir](./purple-garden/src/opt/ir/mod.rs) and
  [opt](./purple-garden/src/opt/bc/mod.rs) modules
- JIT compilation for the whole input by default
- Embeddable with minimal friction for Rust interop via `vm::BuiltinFn`, see
  [help/embed](./help/embed.txt) for a guide and
  [examples/embed-counter/](./examples/embed-counter/) for its real world
  counter part.
- Memory efficient, with an optional garbage collector and a minimal standard
  library, see [std](./purple-garden/src/std/)

## Documentation

For an intro to purple garden see
[help/intro.txt](./purple-garden/help/intro.txt) or run
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

## Editor Setup

Tree-sitter and Neovim LSP setup notes live in
[tree-sitter/README.md](./tree-sitter/README.md).

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
