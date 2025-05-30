## Virtual Machine and Compiler Design

> Most of these examples and debugging output can be accessed via
> `+disassemble` or builing and running purple garden in debug mode via
> `-DDEBUG=1` (a simple `make` runs the debug target)

### Calling conventions

> These apply to both calling purple garden functions, jit compiled native
> functions and builtins

- The amount of arguments is encoded before the callsite via the `OP_ARGS` argument
- Upon hitting `OP_ARGS` value is stored in `Vm::arg_count`

#### Builtins

- builtins are resolved to an index into `builtin::BUILTIN_MAP` by their name
  at compile time
- function pointer at that index is called with 1-n arguments

#### Zero arguments

| Variant       | Behaviour                                                 |
| ------------- | --------------------------------------------------------- |
| `@<builtin>`  | invocation removed in the compiler, no bytecode generated |
| `@function`   | vm jumps to definition                                    |
| jit functions | vm invokes function                                       |

For instance:

```raket
(@println)
```

```asm
globals:
        False; {idx=0}
        True; {idx=1}

entry:
```

#### Single arguments

- `r0` is passed to the function
- function is called

```raket
(@println "Hello World")
```

```asm
__globals:
        False; {idx=0}
        True; {idx=1}
        Str(`Hello`); {idx=2,hash=1847627}
        Str(`World`); {idx=3,hash=2157875}

__entry:
        LOAD 2: Str(`Hello`)
        PUSH 0
        LOAD 3: Str(`World`)
        ARGS 2
        BUILTIN 1: <@println>
```

#### n-Arguments

- Passing n arguments to any type of function works by (example with builtins):
  1. the compiler knows how many arguments are used at the callsite
  2. the compiler compiles each argument to bytecode and afterwards emits `OP_PUSH` to push the result into the vm call argument stack (`Vm::stack`)
  3. the compiler emits `OP_ARGS` and `n` (argument count) and `OP_BUILTIN` `b` (builtin index)
  4. the vm encounters `OP_PUSH`, pushes into the stack, increments stack counter
  5. the vm encounters `OP_ARGS` and modifies `Vm::arg_count` (default is 1)
  6. the vm encounteres `BUILTIN`, pops `n` elements of the stack and passes them to `builtin::BUILTIN_MAP[b]()`

```raket
(@println "Hello" "World" 3.1415 127)
```

```asm
__globals:
        False; {idx=0}
        True; {idx=1}
        Str(`Hello`); {idx=2,hash=1847627}
        Str(`World`); {idx=3,hash=2157875}
        Double(3.1415); {idx=4}
        Int(127); {idx=5}

__entry:
        LOAD 2: Str(`Hello`)
        PUSH 0
        LOAD 3: Str(`World`)
        PUSH 0
        LOAD 4: Double(3.1415)
        PUSH 0
        LOAD 5: Int(127)
        ARGS 4
        BUILTIN 2: <@println>
```
