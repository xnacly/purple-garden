# purple_garden

purple_garden is a lean lisp, designed and implemented with a focus on
performance

```racket
(@function greeting [greetee] 
    (@println "hello world to:" greetee))
; prints `hello world to: user`
(greeting "user")
```

## Local Setup

> purple garden is a C project, so you will need a C compiler toolchain and
> make if you want to following along or you could use the flake :)

```sh
$ git clone git@github.com:xNaCly/purple-garden.git
# or
$ git clone https://github.com/xNaCly/purple-garden.git
```

```sh
# flake flake, if you want
nix develop

# by default purple_garden fills $PG to be ./examples/hello-world.garden
make

# results in:
# ================== INPUTS ==================
# ; vim: filetype=racket
# 
# ; @println is a predefined function responsible for writing to stdout
# ; builtins are specifically called via @<builtin>
# (@println "Hello" "World")
# ================== TOKENS ==================
# [T_DELIMITOR_LEFT]
# [T_BUILTIN][println]
# [T_STRING][Hello]
# [T_STRING][World]
# [T_DELIMITOR_RIGHT]
# [T_EOF]
# lex: 32768.09KB of 598016.00KB used (5.48%)
# ================== ASTREE ==================
# N_BUILTIN[T_BUILTIN][println](
#  N_ATOM[T_STRING][Hello],
#  N_ATOM[T_STRING][World]
# )
# parse: 40960.31KB of 598016.00KB used (6.85%)
# ================== DISASM ==================
# ; vim: filetype=asm
# ; Vm {global=4/4194304, bytecode=10/4194304}
# __globals:
#         False; {idx=0}
#         True; {idx=1}
#         Str(`Hello`); {idx=2,hash=1847627}
#         Str(`World`); {idx=3,hash=2157875}
# 
# __entry:
#         LOAD 2: Str(`Hello`)
#         PUSH 0
#         LOAD 3: Str(`World`)
#         ARGS 2
#         BUILTIN 2: <@println>
# cc: 221216.38KB of 598016.00KB used (36.991715%)
# [VM]: entering frame #1
# ================== GLOBAL ==================
# VM[glob1/4] False
# VM[glob2/4] True
# VM[glob3/4] Str(`Hello`)
# VM[glob4/4] Str(`World`)
# ================== VM OPS ==================
# VM[000000][LOAD    ][         2]: {.registers=[ Str(`Hello`) ]}
# VM[000002][PUSH    ][         0]: {.registers=[ Str(`Hello`) ],.stack=[ Str(`Hello`) ]}
# VM[000004][LOAD    ][         3]: {.registers=[ Str(`World`) ],.stack=[ Str(`Hello`) ]}
# VM[000006][ARGS    ][         2]: {.registers=[ Str(`World`) ],.stack=[ Str(`Hello`) ]}
# Hello World
# VM[000008][BUILTIN ][         2]: {.registers=[ Option(None) ]}
# ==================  REGS  ==================
# VM[r0]: Option(None)
# ================== MEMORY ==================
# cc: 221732.38KB of 598016.00KB used (37.078000%)
# vm: 221732.38KB of 598016.00KB used (37.078000%)

# provide a custom file to execute
make PG=examples/ops.garden
```

### Release builds

> produces a ./purple_garden binary with versioning information and optimisations

```sh
$ make release
./purple_garden
# error: Missing a file? try `-h/--help`
$ ./purple_garden -h
# usage ./purple_garden: [ +v / +version] [ +d / +disassemble]
#                        [ +b / +block-allocator <long=0>] [ +a / +aot-functions]
#                        [ +m / +memory-usage] [ +V / +verbose]
#                        [ +r / +run <string=``>]
#                        [ +h / +help] <file.garden>
# 
# Option:
#           +v / +version
#                 display version information
# 
#           +d / +disassemble
#                 readable bytecode representation with labels, globals and comments
# 
#           +b / +block-allocator <long=0>
#                 use block allocator with size instead of garbage collection
# 
#           +a / +aot-functions
#                 compile all functions to machine code
# 
#           +m / +memory-usage
#                 display the memory usage of parsing, compilation and the virtual machine
# 
#           +V / +verbose
#                 verbose logging
# 
#           +r / +run <string=``>
#                 executes the argument as if an input file was given
# 
#           +h / +help
#                 help page and usage
# 
# Examples:
#         ./purple_garden +v +d \
#                         +b 0 +a \
#                         +m +V \
#                         +r ""
# 
#         ./purple_garden +version +disassemble \
#                         +block-allocator 0 +aot-functions \
#                         +memory-usage +verbose \
#                         +run ""
```

### Running tests

```sh
$ make test
# [+][PASS][Case 1/34] in=`3.1415`
# [+][PASS][Case 2/34] in=`.1415`
# [+][PASS][Case 3/34] in=`"string"`
# [+][PASS][Case 4/34] in=`true false`
# [+][PASS][Case 5/34] in=`true false true false`
# [+][PASS][Case 6/34] in=`"hello"`
# [+][PASS][Case 7/34] in=`(+2 2)`
# [+][PASS][Case 8/34] in=`(-5 3)`
# [+][PASS][Case 9/34] in=`(*3 4)`
# [+][PASS][Case 10/34] in=`(/ 6 2)`
# [+][PASS][Case 11/34] in=`(+1(-2 1))`
# [+][PASS][Case 12/34] in=`(+2.0 2)`
# [+][PASS][Case 13/34] in=`(+2 2.0)`
# [+][PASS][Case 14/34] in=`(-5.0 3)`
# [+][PASS][Case 15/34] in=`(-5 3.0)`
# [+][PASS][Case 16/34] in=`(*3.0 4)`
# [+][PASS][Case 17/34] in=`(*3 4.0)`
# [+][PASS][Case 18/34] in=`(/ 6.0 2)`
# [+][PASS][Case 19/34] in=`(/ 6 2.0)`
# [+][PASS][Case 20/34] in=`(@len "hello")`
# [+][PASS][Case 21/34] in=`(@len "hello")(@len "hello")`
# [+][PASS][Case 22/34] in=`(@len "")`
# [+][PASS][Case 23/34] in=`(@len "a")`
# [+][PASS][Case 24/34] in=`(= 1 1)`
# [+][PASS][Case 25/34] in=`(= "abc" "abc")`
# [+][PASS][Case 26/34] in=`(= 3.1415 3.1415)`
# [+][PASS][Case 27/34] in=`(= true true)`
# [+][PASS][Case 28/34] in=`(= false false)`
# [+][PASS][Case 29/34] in=`(@assert true)`
# [+][PASS][Case 30/34] in=`(@let name "user")`
# [+][PASS][Case 31/34] in=`(@let name "user")name`
# [+][PASS][Case 32/34] in=`(@let age 25)age`
# [+][PASS][Case 33/34] in=`(@function ret[arg] arg)(ret 25)`
# [+][PASS][Case 34/34] in=`(@function add25[arg](+arg 25))(add25 25)`
# 34 of 34 passed, 0 failed
```


Tests are located in `tests/test.c` and a test is declared via the `CASE` macro:

```c
    CASE(3.1415, BC(OP_LOAD, 2), VAL(.type = V_NUM, .number = 3.1415)),
    CASE(.1415, BC(OP_LOAD, 2), VAL(.type = V_NUM, .number = 0.1415)),
    CASE("string", BC(OP_LOAD, 2), VAL(.type = V_STRING, .string = STRING("string"))),
```

### Disassembling bytecode

```sh
./purple_garden +disassemble <file.garden>
```

For readable bytecode representation with labels, globals and comments.

> `+disassemble` is enabled by default when in debug builds via `-DDEBUG=1`

```sh
$ ./purple_garden +disassemble examples/hello-world.garden
# [...] omitted - see below
# Hello World
```

Results in `Hello World` and of course bytecode disassembly:

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
        BUILTIN 2: <@println>
```

Or of course the benchmark example:

```asm
__globals:
        False; {idx=0}
        True; {idx=1}
        Str(`b`); {idx=2,hash=798181}
        Str(`a`); {idx=3,hash=796972}
        Double(2.5); {idx=4}

__entry:
__0x000000[00B2]: comparer
        JMP 26
        STORE 1
        LOAD 2: Str(`b`)
        VAR 1
        POP
        STORE 1
        LOAD 3: Str(`a`)
        VAR 1
        LOADV 796972: $a
        STORE 1
        LOADV 798181: $b
        EQ 1
        BUILTIN 1: <@assert>
        LEAVE


__0x00001C[0047]: inc
        JMP 46
        STORE 1
        LOAD 3: Str(`a`)
        VAR 1
        LOADV 796972: $a
        PUSH 0
        LOADV 796972: $a
        ARGS 2
        CALL 0: <comparer>
        LEAVE

        LOAD 4: Double(2.5)
        CALL 28: <inc>
```

The disassembler attempts to display as much information as possible:

- allocation informations `Vm {<field>=<actual elements>/<allocated element space>}`
- elements of the global pool, their pool index and their hashes
- readable bytecode names: `LOAD` and `BUILTIN` instead of `0` and `6`
- global pool values for certain bytecode operators: ```global=Str(`Hello World`)```
- names for builtin calls: `builtin=@println`
- labels for function definitions `<function>:` and branching `if:`, `then:`, `match:`, `default:`
- names for arguments, functions and variabels

### Benchmarks

For benchmarking, remember to create a large sample size via the purple garden source code:

```sh
$ wc -l examples/bench.garden
# 250003 examples/bench.garden
```

> This benchmark example is for optimizing tail calls and builtin dispatch:

```racket
(@function comparer [a b] (@assert (= a b)))
(@function inc [a] (comparer a a))
(inc 2.5)
(inc 2.5)
(inc 2.5)
(inc 2.5)
; [...]
```

Running the whole thing with `make bench`, the time took for each stage is
notated between `[` and `]`.

```sh
# built in time measurements
$ make bench PG=examples/bench.garden
# [    0.0000ms] main::Args_parse: Parsed arguments
# [    0.0110ms] io::IO_read_file_to_string: mmaped input of size=2500090B
# [    0.0040ms] mem::init: Allocated memory block of size=612368384B
# [   13.5170ms] lexer::Lexer_all: lexed tokens count=1000033
# [    8.2350ms] parser::Parser_next created AST with node_count=250003
# [    6.8260ms] cc::cc: Flattened AST to byte code/global pool length=1000052/250005
# [   20.5990ms] vm::Vm_run: executed byte code
# [    0.4510ms] mem::Allocator::destroy: Deallocated memory space
# [    0.0000ms] vm::Vm_destroy: teared vm down
# [    0.0000ms] munmap: unmapped input

# or hyperfine
$ make release
$ hyperfine "./purple_garden examples/bench.garden"
# Benchmark 1: ./purple_garden examples/bench.garden
#   Time (mean ± σ):      49.0 ms ±   0.8 ms    [User: 40.5 ms, System: 7.9 ms]
#   Range (min … max):    48.0 ms …  52.6 ms    60 runs
```

### Profiling

Using perf and [hotspot](https://github.com/KDAB/hotspot), you can get a
flamechart and other info:

```sh
$ make release
$ perf record --call-graph dwarf ./purple_garden ./bench.garden
# just top
$ perf report
# flamegraph
$ hotspot
```

## Features

> Documentation for these is a work in progress, since the language is still
> subject to a lot of changes

- [ ] data types
  - [x] numbers
  - [x] strings
  - [x] booleans
  - [x] lists
  - [ ] optionals (support in backend - compiler, vm)
  - [ ] objects
- [ ] language constructs
  - [x] variables
  - [ ] if
  - [ ] match
  - [ ] loops
  - [x] functions
  - [ ] pattern matching
- [ ] builtins
  - [x] println
  - [x] print
  - [x] len
  - [x] type
  - [x] assert

## Optimisations

- [x] `lexer`,`parser`, `cc`: separate memory regions and allocation strategies
  from `vm`
- [x] `lexer`, `cc`, `vm`: hash idents at lex time, use hash for variable access
- [x] `vm`: register based, instead of stack based
- [x] `cc`: fastpaths for zero argument builtin stripping
- [x] `cc`: fastpaths for one argument call sites
- [x] `lexer`: computed goto jump table
- [x] `io`: `mmap` instead of reading the input file manually
- [x] `common`: turn `String` for `char*` abstraction into windows instead of allocating in the lexer
- [x] `parser`: use bump allocator for node children
- [x] `lexer`: zero copy identifiers, strings, numbers and booleans
- [x] `lexer`: fastpath for non floating point numbers
- [x] `lexer`: fastpath for boolean identification
- [x] `lexer+parser+cc`: no dynamic allocations by using bump allocator
- [x] `cc`: zero argument builtin calls and operations are skipped
- [x] `cc`: bump allocator for globals and bytecode
- [x] `cc`: replace `@<builtin>` calls with indexes into `builtin::BUILTIN_MAP` to move function lookup from runtime to compile time
- [x] `cc`: compute `@<builtin>` indexes for identifiers via hash compare with precomputed builtin hashes
- [x] `cc`: intern strings and identifiers to deduplicate `Vm::globals`
- [x] `cc`: single instances for `true` and `false` in the global pool
- [x] `cc`: hash known identifiers and strings at compile time
- [x] `cc`: fast path for `ADD,SUB,MUL,DIV` with one child and a fast path for two children
- [x] `cc`, `vm`: operate only on references, not values
- [x] `vm`: frame free list to make entering and leaving scopes as fast as possible
- [x] `vm`: preallocate 256 frames for even faster scope interaction
- [x] `vm`: `EQ` fastpath for checking if lhs and rhs point to the same memory region
- [ ] `jit`: native compile functions before enterning the runtime, enabled via `+aot-functions`
- [ ] `cc`: multiple string concatinations should use a shared buffer and only allocate on string usage
- [ ] `vm`: trail call optimisation
- [ ] `vm`: merge smaller bytecode ops often used together into new ops
- [ ] `vm`: lock I/O for the whole program execution for faster performance via `+lock-io`
- [ ] `cc`: cache bytecode and global pool to omit frontend, disable via `+no-cache`
- [ ] `gc`: mark and sweep garbage collection via `+gc-marksweep`
- [ ] `gc`: generational garbage collection via `+gc-gen`
- [ ] `gc`: reference counting via `+gc-rc`
- [x] `gc`: allow for bump/block allocator with `+block-allocator`
