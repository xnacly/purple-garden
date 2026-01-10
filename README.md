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

## Setup
