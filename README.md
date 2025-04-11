# purple_garden

purple_garden is a lean lisp, designed and implemented with a focus on
performance

```racket
(@function greeting (greetee)
    (+ "hello world to: " greetee))
(@println (greeting "user"))
; prints `hello world to: user`
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
# lex: 131072.00KB of 729088.00KB used (17.98%)
# ================== ASTREE ==================
# N_BUILTIN[T_BUILTIN][println](
#  N_ATOM[T_STRING][Hello],
#  N_ATOM[T_STRING][World]
# )
# parse: 188416.27KB of 729088.00KB used (25.84%)
# ================== DISASM ==================
# ; vim: filetype=asm
# ; Vm {global=4/4194304, bytecode=12/4194304}
# globals:
#         False; {idx=0}
#         True; {idx=1}
#         Str(`Hello`); {idx=2,hash=1847627}
#         Str(`World`); {idx=3,hash=2157875}
# 
# entry:
#         ; [op=0,arg=2] at (0/1)
#         ; global=Str(`Hello`)
#         LOAD 2
#         ; [op=1,arg=1] at (2/3)
#         STORE 1
#         ; [op=0,arg=3] at (4/5)
#         ; global=Str(`World`)
#         LOAD 3
#         ; [op=7,arg=0] at (6/7)
#         OFFSET 0
#         ; [op=6,arg=2] at (8/9)
#         ARGS 2
#         ; [op=8,arg=1] at (10/11)
#         ; builtin=@println
#         BUILTIN 1
# 
# cc: 331776.27KB of 729088.00KB used (45.505655%)
# ================== MEMORY ==================
# total: 331776.27KB of 729088.00KB used (45.51%)
# ================== GLOBAL ==================
# VM[glob1/4] False
# VM[glob2/4] True
# VM[glob3/4] Str(`Hello`)
# VM[glob4/4] Str(`World`)
# ================== VM OPS ==================
# VM[000000][LOAD    ][  2]: undefined, undefined, undefined
# VM[000002][STORE   ][  1]: Str(`Hello`), undefined, undefined
# VM[000004][LOAD    ][  3]: Str(`Hello`), Str(`Hello`), undefined
# VM[000006][OFFSET  ][  0]: Str(`World`), Str(`Hello`), undefined
# VM[000008][ARGS    ][  2]: Str(`World`), Str(`Hello`), undefined
# VM[000010][BUILTIN ][  1]: Str(`World`), Str(`Hello`), undefined
# Hello World
# ==================  REGS  ==================
# VM[r0]: Option(None)
# VM[r1]: Str(`Hello`)
# VM[r2]: undefined

# provide a custom file to execute
make PG=examples/ops.garden
```

### Release builds

> produces a ./purple_garden binary with versioning information and optimisations

```sh
$ make release
./purple_garden
# usage: purple_garden [-v | --version] [-h | --help] [-d | --disassemble]
#                      [-b | --block-allocator] [-a | --aot-functions] [-m | --memory-usage]
#                      [-V | --verbose] <file.garden>
# error: Missing a file? try `-h/--help
$ ./purple_garden -h
# usage: purple_garden [-v | --version] [-h | --help] [-d | --disassemble]
#                      [-b | --block-allocator] [-a | --aot-functions] [-m | --memory-usage]
#                      [-V | --verbose] <file.garden>
# 
# options:
#         -v, --version         display version information
#         -h, --help            extended usage information
#         -d, --disassemble     readable bytecode representation with labels, globals and comments
#         -b, --block-allocator use block allocator instead of garbage collection
#         -a, --aot-functions   compile all functions to machine code
#         -m, --memory-usage    display the memory usage of parsing, compilation and the virtual machine
#         -V, --verbose         verbose logging
```

### Running tests

```sh
$ make test
# [+][PASS][Case 1/14] in=`3.1415`
# [+][PASS][Case 2/14] in=`.1415`
# [+][PASS][Case 3/14] in=`"string"`
# [+][PASS][Case 4/14] in=`true false`
# [+][PASS][Case 5/14] in=`true false true false`
# [+][PASS][Case 6/14] in=`"hello"`
# [+][PASS][Case 7/14] in=`(+2 2)`
# [+][PASS][Case 8/14] in=`(-5 3)`
# [+][PASS][Case 9/14] in=`(*3 4)`
# [+][PASS][Case 10/14] in=`(/ 6 2)`
# [+][PASS][Case 11/14] in=`(@len "hello")`
# [+][PASS][Case 12/14] in=`(@len "hello")(@len "hello")`
# [+][PASS][Case 13/14] in=`(@len "")`
# [+][PASS][Case 14/14] in=`(@len "a")`
# 14 of 14 passed, 0 failed
```


Tests are located in `tests/test.c` and a test is declared via the `CASE` macro:

```c
    CASE(3.1415, BC(OP_LOAD, 2), VAL(.type = V_NUM, .number = 3.1415)),
    CASE(.1415, BC(OP_LOAD, 2), VAL(.type = V_NUM, .number = 0.1415)),
    CASE("string", BC(OP_LOAD, 2), VAL(.type = V_STRING, .string = STRING("string"))),
```

### Disassembling bytecode

```sh
./purple_garden --disassemble <file.garden>
```

For readable bytecode representation with labels, globals and comments.

> `--disassemble` is enabled by default when in debug builds via `-DDEBUG=1`

```sh
$ ./purple_garden --disassemble examples/hello-world.garden
# [...] omitted - see below
# Hello World
```

Results in `Hello World` and of course bytecode disassembly:

```asm
; vim: filetype=asm
; Vm {global=4/4194304, bytecode=12/4194304}
globals:
        False; {idx=0}
        True; {idx=1}
        Str(`Hello`); {idx=2,hash=1847627}
        Str(`World`); {idx=3,hash=2157875}

entry:
        ; [op=0,arg=2] at (0/1)
        ; global=Str(`Hello`)
        LOAD 2
        ; [op=1,arg=1] at (2/3)
        STORE 1
        ; [op=0,arg=3] at (4/5)
        ; global=Str(`World`)
        LOAD 3
        ; [op=7,arg=0] at (6/7)
        OFFSET 0
        ; [op=6,arg=2] at (8/9)
        ARGS 2
        ; [op=8,arg=1] at (10/11)
        ; builtin=@println
        BUILTIN 1
```

The disassembler attempts to display as much information as possible:

- allocation informations `Vm {<field>=<actual elements>/<allocated element space>}`
- elements of the global pool, their pool index and their hashes
- bytecode operator and argument values and indexes: `[op=6,arg=0] at (0/1)`
- readable bytecode names: `LOAD` and `BUILTIN` instead of `0` and `6`
- global pool values for certain bytecode operators: ```global=Str(`Hello World`)```
- names for builtin calls: `builtin=@println`
- labels for function definitions `<function>:` and branching `if:`, `then:`, `match:`, `default:`

### Benchmarks

For benchmarking, remember to create a large sample size via the purple garden source code:

```sh
$ wc -l examples/bench.garden
# 250001 examples/bench.garden
```

> This benchmark is for optimizing `builtin_len`/`@len` calls and atom
> interning:

```racket
(@len "hello world")
(@len "hello world")
(@len "hello world")
(@len "hello world")
(@len "hello world")
; [...]
```

Running the whole thing with `make bench`, the time took for each stage is
notated between `[` and `]`.

```sh
# built in time measurements
$ make bench PG=examples/bench.garden
# [    0.0000ms] main::Args_parse: Parsed arguments
# [    0.0110ms] io::IO_read_file_to_string: mmaped input of size=5250021B
# [    0.0080ms] mem::init: Allocated memory block of size=934503738B
# [   13.3990ms] lexer::Lexer_next lexed tokens count=1000005
# [   11.2210ms] parser::Parser_next created AST with node_count=1000005
# [    5.8020ms] cc::cc: Flattened AST to byte code/global pool length=1500006/3
# [    2.6640ms] vm::Vm_run: executed byte code
# [    0.6080ms] mem::Allocator::destroy: Deallocated memory space
# [    0.0000ms] vm::Vm_destroy: teared vm down
# [    0.3120ms] munmap: unmapped input

# or hyperfine
$ make release
$ hyperfine "./purple_garden examples/bench.garden"
# Benchmark 1: ./purple_garden examples/bench.garden
#   Time (mean ± σ):      34.0 ms ±   0.8 ms    [User: 25.6 ms, System: 8.1 ms]
#   Range (min … max):    32.8 ms …  36.8 ms    85 runs
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
  - [ ] variables
  - [ ] if
  - [ ] match
  - [ ] loops
  - [ ] functions
  - [ ] pattern matching
- [ ] builtins
  - [x] println
  - [x] print
  - [x] len
  - [ ] hash

## Optimisations

- [x] `vm`: register based instead of stack based
- [x] `io`: `mmap` instead of reading the input file manually
- [x] `common`: turn `String` for `char*` abstraction into windows instead of allocating in the lexer
- [x] `parser`: use bump allocator for node children
- [x] `lexer`: zero copy identifiers, strings, numbers and booleans
- [x] `lexer`: fastpath for non floating point numbers
- [x] `lexer`: fastpath for boolean identification
- [x] `lexer+parser+cc`: no dynamic allocations by using bump allocator
- [x] `cc`: bump allocator for globals and bytecode
- [x] `cc`: replace `@<builtin>` calls with indexes into `builtin::BUILTIN_MAP` to move function lookup from runtime to compile time
- [x] `cc`: compute `@<builtin>` indexes for identifiers via hash compare with precomputed builtin hashes
- [x] `cc`: intern strings and identifiers to deduplicate `Vm::globals`
- [x] `cc`: single instances for `true` and `false` in the global pool
- [x] `cc`: hash known identifiers and strings at compile time
- [x] `cc`: fast path for `ADD,SUB,MUL,DIV` with one child and a fast path for two children
- [ ] `jit`: native compile functions before enterning the runtime, enabled via `--aot-functions`
- [ ] `cc`: multiple string concatinations should use a shared buffer and only allocate on string usage
- [ ] `vm`: trail call optimisation
- [ ] `vm`: merge smaller bytecode ops often used together into new ops
- [ ] `vm`: lock I/O for the whole program execution for faster performance via `--lock-io`
- [ ] `cc`: cache bytecode and global pool to omit frontend, disable via `--no-cache`
- [ ] `gc`: mark and sweep garbage collection via `--gc-marksweep`
- [ ] `gc`: generational garbage collection via `--gc-gen`
- [ ] `gc`: reference counting via `--gc-rc`
- [ ] `gc`: allow for bump/block allocator with `--alloc-block`
