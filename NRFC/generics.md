# Generics

All generics, be it specialisation or slots are not exposed to the user in the
sense of creating them, they can however be consumed by users. 

Embedders can mark functions as either specialising (choosing an implementation
based on the type) or generic, which accepts a list of generic types used in
said function.

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

## Monomorphism

However, this only works for known types requiring different implementations.
It does not work for something like `cmp.or(bool,T) Option<T>`, which decided
on bool if T is wrapped and passed as an optional back out of the function. It
needs to do so for all possible inputs, and we cant write a specialisation for
types we may now know, something like `Record<Age:Int Name:Str>` shows the
unlimited amount of work specialising every possible T for every stdlib
function would require. Therefore purple garden needs substitution based
generics (called slots), which enable passing a value through something and
"specialising" the function at compile time for the known input automatically.

Given the identity function:

```rust
#[pg_pkg(runtime = purple_garden_runtime)]
pub mod t {
    /// lx.x
    #[pg_fn(with_slots)]
    pub fn id(x: embed::Slot("T")) -> embed::Slot("T") {
        x
    }
}
```

Producing the following after macro expansion:

```rust
unsafe extern "C" fn id(vm: *mut std::ffi::c_void) {
    // This could be an empty body, but the macro codegen does not optimise,
    // thus the id function is taken verbatim
    let vm = unsafe { &mut *vm.cast::<Vm>() };
    let inner = vm.r(0);
    *vm.r_mut(0) = *inner;
}

pub const PACKAGE: purple_garden_runtime::embed::Pkg = purple_garden_runtime::embed::Pkg {
    name: "t",
    doc: "t",
    pkgs: &[],
    fns: &[purple_garden_runtime::embed::Fn {
        name: "id",
        doc: "lx.x",
        ptr: id,
        pure: false,
        eval: None,
        with_slots: true,
        arg_names: &["x"],
        args: &[Type::Slot("T")],
        ret: Type::Slot("T"),
        specialises: None,
    }],
};
```

For the below example usage from pgs side there should be a generated function for each of the different usages (in both IR, pgvm and x86)

```garden
import "t"

t.id(3.1415)         # Double
t.id(3)              # Int
t.id("hello")        # Str
t.id(true)           # Bool
t.id({ n:9 m:10 })   # Record<n:Int m:Int>
t.id(["ab" "bc"])    # Array<Str>
```

```text
$ cargo run --features trace -- -ITT test.garden
call: Double
  callee t.id
  3.1415: Double
...
call: Record<n: Int m: Int>
  callee t.id
  record: Record<n: Int m: Int>
    field n
      9: Int
    field m
      10: Int
call: Array<Str>
  callee t.id
  array: Array<Str>
    ab: Str
    bc: Str
```

```llvmir
// entry
fn f0() -> Void {
b0():
        %v0:Double = 3.1415
        %v1:Double = Sys t.id(%v0)
        ...
        %v8 = Alloc Record<n: Int m: Int>(size=16,align=8)
        %v9:Int = 9
        Store %v8+0, %v9
        %v10:Int = 10
        Store %v8+8, %v10
        %v11:Record<n: Int m: Int> = Sys t.id(%v8)
        %v12 = Alloc Array<Str>(size=24,align=8)
        %v13:Int = 2
        Store %v12+0, %v13
        %v14:Str = `ab`
        Store %v12+8, %v14
        %v15:Str = `bc`
        Store %v12+16, %v15
        %v16:Array<Str> = Sys t.id(%v12)
        ret %v16
}
```

```asm
00000000 <entry>:
  0000:    load_global r0, 0         ; 2: t.id(3.1415)         # Double
  0001:    sys 0 <t.id>
  .....
  0008:    alloc r0, Record, #16, #8 ; 6: t.id({ n:9 m:10 })   # Record<n:Int m:Int>
  0009:    load_imm r1, #9
  000a:    store r0, #0, r1
  000b:    load_imm r1, #10
  000c:    store r0, #8, r1
  000d:    sys 0 <t.id>
  000e:    alloc r0, Array, #24, #8  ; 7: t.id(["ab" "bc"])    # Array<Str>
  000f:    load_imm r1, #2
  0010:    store r0, #0, r1
  0011:    load_global r1, 2
  0012:    store r0, #8, r1
  0013:    load_global r1, 3
  0014:    store r0, #16, r1
  0015:    sys 0 <t.id>
  0016:    ret
  0017:    halt
```
