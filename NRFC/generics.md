# Generics

## Specialisation based

Things like 

```garden
import ("io")

io.println("Hello World")
```

Work by defining println with a list of specialisations, for `io.println`:

```rust
#[pg_pkg(runtime = purple_garden_runtime)]
pub mod io {
    #[pg_fn(specialises = "println")]
    pub fn println_str(s: &str) {
        println!("{s}");
    }

    #[pg_fn(specialises = "println")]
    pub fn println_int(i: i64) {
        println!("{i}");
    }

    #[pg_fn(specialises = "println")]
    pub fn println_double(d: f64) {
        println!("{d}");
    }
}
```

The lowering step from AST->IR then picks the implementation fitting to the
types passed into the function: 

```text
$ cargo run --features trace -- -TT test.garden
...
[          78.496us] [ir::typecheck::Typechecker::node] resolved pkg `io`
[         101.004us] [ir::typecheck::Typechecker::node] resolved `io.println` to specialisation 1/3 (Str) -> Void
import io
call: Void
  callee io.println
  Hello World: Str
...
```

This specialisation is then used in both the IR:

```llvm-ir
// entry
fn f0() -> Void {
b0():
        %v0:Str = `Hello World`
        %v1:Void = Sys io.println_str(%v0)
        ret %v1
}
```

And the bytecode:

```asm
globals:
  0000:    `Hello World`

00000000 <entry>:
  0000:    load_global r0, 0      ; 3: io.println("Hello World")
  0001:    sys 0 <io.println_str>
  0002:    ret
```

## Substitution based

However, this only works for known types requiring different implementations.
It does not work for something like `cmp.or(bool,T) Option<T>`, which decided
on bool if T is wrapped and passed as an optional back out of the function. It
needs to do so for all possible inputs, and we cant write a specialisation for
types we may now know, something like `Record<Age:Int Name:Str>` shows the
unlimited amount of work specialising every possible T for every stdlib
function would require. Therefore purple garden needs substitution based
generics, which enable passing a value through something and "specialising" the
function at compile time for the known input automatically.
