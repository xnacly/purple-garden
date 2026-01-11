# purple_garden

purple_garden is a lean scripting language, designed and implemented with a
focus on performance via strategies like aggressive compile time optimisations,
just in time compilation for runtime hotspots, fine grained control over memory
and gc, while allowing to disable the gc and stdlib fully.

```python
fn greeting :: greetee {
    std::println("hello world to:" greetee)
} 
greeting(std::env::get("USER")) # hello world to: $USER

fn tuplify :: v {
    [type(v) len(v)]
} 
std::println(tuplify("hello world")) # [str, 11]
```

## Features / Design goals

- be as fast as I can get it to be; which is very fast
- do the first, but be safe
- embeddability, low friction for interop with rust
- be memory efficient

## Local Setup

> Since purple garden is a rust project, it requires cargo, it also requires
> nightly, due to its usage of branch predictions in the virtual machine

```shell
git clone git@github.com:xnacly/purple-garden.git
cargo +nightly run -- --help
```
