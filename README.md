# purple_garden

purple_garden is a lean scripting language designed for performance, with a
focus on aggressive compile-time optimisations, JIT compilation, fine-grained
memory control, and optional garbage collection.

```python
fn greeting(greetee: str) {
    std::println("hello world to:" greetee)
} 
greeting(std::env::get("USER"))  # hello world to: $USER
```

## Features / Design Goals

- Extremely fast execution with a register-based VM and aggressive compile-time
  optimisations (both IR and peephole)
- JIT compilation for runtime hotspots
- Embeddable with minimal friction for Rust interop
- Memory efficient, with an optional garbage collector and a minimal standard library

## Local Setup

> Nightly Rust is required due to branch prediction optimisations in the VM.

```bash
git clone git@github.com:xnacly/purple-garden.git
cargo +nightly run -- --help
```

## Architecture

```text
.
+- Tokenizer
|
]: Token(2) Token(+) Token(3)
]: Token(*)
]: Token(4) Token(-) Token(1)
|
 \
  +- Parsing (Tokens -> Abstract Syntax Tree)
  |
  ]: (Asteriks
  ]:   (Plus
  ]:     Integer("2")
  ]:     Integer("3")
  ]:   )
  ]:   (Minus
  ]:     Integer("4")
  ]:     Integer("1")
  ]:   )
  ]: )
  |
  |
<planned section start>
  \
   +- Planned IR and Optimisation Boundary
   |
  / \
  |  +- JIT Compiler (IR -> x86/ARM)
  |                           ^
  |                            \
  |                             \
  |                              \ 
  |                               \ 
  |                                \ 
<planned section end>               |Calls 
  |                                 |JIT'ed    
  \                                 |functions 
   +- Compiler (AST/IR -> bytecode) |
   |                                / 
   ]:  __entry:                    /
   ]:          load_imm r0, #2    |
   ]:          load_imm r1, #3    |
   ]:          add r2, r0, r1     |
   ]:          load_imm r1, #4    |
   ]:          load_imm r0, #1    |
   ]:          sub r3, r1, r0     |
   ]:          mul r0, r2, r3     |
   |                              |
   \                              |
    +- Peephole Optimiser         |
    |                             |
    ]:  __entry:                  |
    ]:          load_imm r2, #5   |
    ]:          load_imm r3, #3   |
    ]:          mul r0, r2, r3    |
    |                            /
    \                           /
     +- Baseline interpreter --+
     |
     ]: [vm][0000] LoadImm { dst: 2, value: 5 }
     ]: [vm][0001] LoadImm { dst: 3, value: 3 }
     ]: [vm][0002] Mul { dst: 0, lhs: 2, rhs: 3 }
     |
     '
```
