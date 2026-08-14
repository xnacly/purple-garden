# Performance

## local Common subexpression elimination (lCSE) 

> See [Common subexpression elimination -
> wikipedia](https://en.wikipedia.org/wiki/Common_subexpression_elimination)

Best explained with the mandelbrot example:

```rust
#! Return the iteration at which one point escapes, or `max` if it remains bounded.
fn mandel_iter(zr:Double zi:Double cr:Double ci:Double i:Int max:Int) Int {
    match {
        i == max { max }
        zr*zr + zi*zi > 4.0 { i }
        { mandel_iter(zr*zr - zi*zi + cr  2.0*zr*zi + ci  cr  ci  i+1  max) }
    }
}
```

Here `zr*zr` and `zi*zi` are computed twice, this could
be omitted by "moving this computation to an earlier place" in the execution
order:

```garden
#! Return the iteration at which one point escapes, or `max` if it remains bounded.
fn mandel_iter(zr:Double zi:Double cr:Double ci:Double i:Int max:Int) Int {
    let zr2 = zr*zr
    let zi2 = zi*zi
    match {
        i == max { max }
         zr2 + zi2 > 4.0 { i }
        { mandel_iter(zr2 - zi2 + cr  2.0*zr*zi + ci  cr  ci  i+1  max) }
    }
}
```

This should be done as an IR opt pass, meaning common expressions should be
assigned to a free virtual register and instead of recomputing them the pass
should introduce references to the virtual register.


## Exploring computed gotos and tailcalls in the interpreter

> Scrapped, since interpreter doesnt have many mispredections and the jit is
> the solution for dispatch removal

For instance mandelbrot:

```text
4.35 s total, 20 whole-program VM executions

20.93B cycles
89.07B instructions       => 4.26 IPC
15.01B branches
8.92M branch misses       => 0.059% miss rate

472,311 L1-D load misses
6,892 L1-I miss-tagged instructions
1.14M L1-I stall cycles  -> 0.0055% of cycles
```
