# Performance

## Exploring computed gotos and tailcalls in the interpreter

- Rewrite `vm::Vm::run` to use computed goto by indexing into an array of fn ptrs, this will probably require a different op code layout
- Explore the [become](https://doc.rust-lang.org/stable/std/keyword.become.html) proposal

The goal is to make the dispatch in the bytecode interpreter less costly
