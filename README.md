# purple_garden

purple_garden is a lean scripting language designed for performance, with a
focus on aggressive compile-time optimisations, JIT compilation, fine-grained
memory control, and optional garbage collection. It is built to be easily
embedded and extended from Rust.

```garden
import ("io" "str")

io.println(str.concat("Hello" " World"))
```

## Features / Design Goals

- Extremely fast execution with a register-based VM, aggressive compile-time
  optimisations enabled by SSA IR and hardware near bytecode design, see the
  [ir](./purple-garden-opt/src/ir) and
  [opt](./purple-garden-opt/src/bc) modules
- JIT compilation for the whole input by default (can be disabled with `--no-jit`)
- Embeddable with 0 friction for Rust interop; see the example below and
  [help/embed](./help/embed.md) for the complete API.
- Memory efficient, with an optional garbage collector and a minimal standard
  library, see [std](./purple-garden-std/src)
- Editor support via language server protocol implementation and tree-sitter
  grammar, see [tree-sitter/README.md](./tree-sitter/README.md) (VS Code extension
  is a work in progress)

## Embedding

```rust
use std::{collections::HashMap, sync::Mutex};
use purple_garden::{pg_pkg, GardenOpaque, GardenValue, Pg};

#[derive(GardenOpaque)]
struct Store(Mutex<HashMap<String, Profile>>);

#[derive(GardenValue, Clone)]
struct Profile { name: String, plan: String }

#[pg_pkg]
mod store {
    use super::{Profile, Store};

    pub fn put(store: &Store, key: String, value: Profile) {
        store.0.lock().unwrap().insert(key, value);
    }
    pub fn get(store: &Store, key: String, fallback: Profile) -> Profile {
        store.0.lock().unwrap().get(&key).cloned().unwrap_or(fallback)
    }
}

// parse and compile a program from source code with above defined store module
let mut program = Pg::new().with_lib(&store::PACKAGE).compile(br#"
    import "store"
    fn provision(cache: Foreign<Store>) Str {
        store.put(cache "user:42" { name: "Ada" plan: "pro" })
        let profile = store.get(cache "user:42" { name: "n/a" plan: "free" })
        profile.name
    }
"#)?;

let cache = Store(Mutex::new(HashMap::new()));
// locate "provision" function by name and validate types
let provision = program.function::<(&Store,), String>("provision").unwrap();
// call function with a reference to the cache / store
let output = program.call(&provision, (&cache,))?;

assert_eq!(output, "Ada");
assert_eq!(cache.0.lock().unwrap()["user:42"].plan, "pro");
# Ok::<(), Box<dyn std::error::Error>>(())
```

For in depth docs consult the [help/embed](./help/embed.md) guide.

## Documentation

For an intro to purple garden see
[help/intro.md](./help/intro.md) or run
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

### Bytecode Disassembly

Functions the JIT does not (YET) compile, run on the register based virtual
machine. The -D option prints their bytecode, annotated with the source line
each instruction stems from, while jitted functions are called via `sys` and
shown as a hexdump. --no-jit fully disables the jit and thus shows the bytecode
of every function:

```fish
cargo run -- -Dd --no-jit examples/factorial.garden
```

Producing:

```text
globals:
  0000:    2432902008176640000

00000000 <factorial>:
  0000:    push r2                                    ; 6: fn factorial(n:Int a:Int) Int {
  0001:    jmpne_imm r0, #0, 0005 <factorial.bb_0005> ; 8: n == 0 { a }
  0002:    mov r0, r1
  0003:    pop r2
  0004:    ret
00000005 <factorial.bb_0005>:
  0005:    isub_imm r2, r0, #1                        ; 9: { factorial(n-1 n*a) }
  0006:    imul r1, r0, r1
  0007:    mov r0, r2
  0008:    pop r2
  0009:    tail 0000 <factorial>

0000000a <entry>:
  000a:    load_imm r0, #20                           ; 13: testing.assert(factorial(20 1) == 2432902008176640000)
  000b:    load_imm r1, #1
  000c:    call 0000 <factorial>
  000d:    load_global r1, 0
  000e:    ieq r0, r0, r1
  000f:    sys 0 <testing.assert>
  0010:    ret
  0011:    halt
```

### JIT disassembly

Since purple garden gets its main performance from just in time compilation it
includes a -DD option for ELF compatible byte formatting, including relocatable
info for jit internal helpers and standard library calls, invokable via:

```fish
objdump -dr (cargo run -- -DDd examples/factorial.garden | psub)
```

Producing: 

```x86-asm
0000000000000000 <jit_factorial>:
   0:	48 8b 77 00          	mov    0x0(%rdi),%rsi
   4:	4c 8b 47 08          	mov    0x8(%rdi),%r8
   8:	48 85 f6             	test   %rsi,%rsi
   b:	0f 85 0b 00 00 00    	jne    1c <jit_factorial+0x1c>
  11:	4c 89 47 00          	mov    %r8,0x0(%rdi)
  15:	31 c0                	xor    %eax,%eax
  17:	c3                   	ret
  18:	cc                   	int3
  19:	cc                   	int3
  1a:	cc                   	int3
  1b:	cc                   	int3
  1c:	49 89 f1             	mov    %rsi,%r9
  1f:	49 81 e9 01 00 00 00 	sub    $0x1,%r9
  26:	4c 0f af c6          	imul   %rsi,%r8
  2a:	4c 89 ce             	mov    %r9,%rsi
  2d:	e9 d6 ff ff ff       	jmp    8 <jit_factorial+0x8>
  32:	c3                   	ret
```

### Benchmarks

> These may take a while

```bash
set -x RUSTFLAGS "-C target-cpu=native"
cargo bench --workspace
```

For a fast run with less statistical confidence:

```bash
set -x RUSTFLAGS "-C target-cpu=native"
PG_BENCH_QUICK=1 cargo bench --workspace
```

`PG_BENCH_QUICK=1` uses 10 samples, a 100ms warm-up, and a 300ms measurement
window per benchmark.
