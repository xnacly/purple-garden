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
# ; vim: filetype=racket
# 
# ; @println is a predefined function responsible for writing to stdout
# ; builtins are specifically called via @<builtin>
# (@println "Hello" "World")
# [T_DELIMITOR_LEFT]
# [T_BUILTIN][println]
# [T_STRING][Hello]
# [T_STRING][World]
# [T_DELIMITOR_RIGHT]
# [T_EOF]
# lex : 32768.09KB of 149504.00KB used (21.92%)
# N_BUILTIN[T_BUILTIN][println](
#  N_ATOM[T_STRING][Hello],
#  N_ATOM[T_STRING][World]
# )
# ast : 40960.27KB of 149504.00KB used (27.40%)
# allocating r1
# allocating r2
# freeing r2
# freeing r1
# __globals:
#         False; {idx=0}
#         True; {idx=1}
#         Option::None; {idx=2}
#         Str(`Hello`); {idx=3,hash=274763}
#         Str(`World`); {idx=4,hash=60723}
# 
# __entry:
#         LOADG 3: Str(`Hello`)
#         STORE 1
#         LOADG 4: Str(`World`)
#         STORE 2
#         ARGS 2
#         BUILTIN 166
# cc  : 69632.34KB of 149504.00KB used (46.575567%)
# Hello World
# vm  : 70658.34KB of 149504.00KB used (47.261836%)
# | Opcode     | Compiled %               | Executed %               |
# | ---------- | ------------------------ | ------------------------ |
# | STORE      | 2               (33.33%) | 2               (33.33%) |
# | LOADG      | 2               (33.33%) | 2               (33.33%) |
# | ARGS       | 1               (16.67%) | 1               (16.67%) |
# | BUILTIN    | 1               (16.67%) | 1               (16.67%) |
# | ========== | ======================== | ======================== |
# | ::<>       | 6               (99.99%) | 6               (99.99%) |

# provide a custom file to execute
make PG=examples/ops.garden
```

### Release builds

> produces a ./purple_garden binary with versioning information and optimisations

```sh
$ make release
./purple_garden
# error: Missing a file? try `+h/+help`
$ ./purple_garden +h
# usage ./build/purple_garden: [ +v / +version] [ +d / +disassemble]
#                              [ +b / +block-allocator <long=0>] [ +a / +aot-functions]
#                              [ +m / +memory-usage] [ +V / +verbose]
#                              [ +s / +stats] [ +r / +run <string=``>]
#                              [ +h / +help] <file.garden>
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
#           +s / +stats
#                 show statistics
# 
#           +r / +run <string=``>
#                 executes the argument as if an input file was given
# 
#           +h / +help
#                 help page and usage
# 
# Examples:
#         ./build/purple_garden +v +d \
#                               +b 0 +a \
#                               +m +V \
#                               +s +r ""
# 
#         ./build/purple_garden +version +disassemble \
#                               +block-allocator 0 +aot-functions \
#                               +memory-usage +verbose \
#                               +stats +run ""
```

### Running tests

```sh
$ make test
# [+][PASS][Case 1/37] in=`3.1415`
# [+][PASS][Case 2/37] in=`.1415`
# [+][PASS][Case 3/37] in=`"string"`
# [+][PASS][Case 4/37] in=`true false`
# [+][PASS][Case 5/37] in=`true false true false`
# [+][PASS][Case 6/37] in=`"hello"`
# [+][PASS][Case 7/37] in=`(+2 2)`
# [+][PASS][Case 8/37] in=`(-5 3)`
# [+][PASS][Case 9/37] in=`(*3 4)`
# [+][PASS][Case 10/37] in=`(/ 6 2)`
# [+][PASS][Case 11/37] in=`(+1(-2 1))`
# [+][PASS][Case 12/37] in=`(+2.0 2)`
# [+][PASS][Case 13/37] in=`(+2 2.0)`
# [+][PASS][Case 14/37] in=`(-5.0 3)`
# [+][PASS][Case 15/37] in=`(-5 3.0)`
# [+][PASS][Case 16/37] in=`(*3.0 4)`
# [+][PASS][Case 17/37] in=`(*3 4.0)`
# [+][PASS][Case 18/37] in=`(/ 6.0 2)`
# [+][PASS][Case 19/37] in=`(/ 6 2.0)`
# [+][PASS][Case 20/37] in=`(@len "hello")`
# [+][PASS][Case 21/37] in=`(@len "hello")(@len "hello")`
# [+][PASS][Case 22/37] in=`(@len "")`
# [+][PASS][Case 23/37] in=`(@len "a")`
# [+][PASS][Case 24/37] in=`(= 1 1)`
# [+][PASS][Case 25/37] in=`(= "abc" "abc")`
# [+][PASS][Case 26/37] in=`(= 3.1415 3.1415)`
# [+][PASS][Case 27/37] in=`(= true true)`
# [+][PASS][Case 28/37] in=`(= true false)`
# [+][PASS][Case 29/37] in=`(= false false)`
# [+][PASS][Case 30/37] in=`(@let name "user")name`
# [+][PASS][Case 31/37] in=`(@let age 25)age`
# [+][PASS][Case 32/37] in=`(@function ret[arg] arg)(ret 25)`
# [+][PASS][Case 33/37] in=`(@function add25[arg](+arg 25))(add25 25)`
# [+][PASS][Case 34/37] in=`(@assert true)`
# [+][PASS][Case 35/37] in=`(@None)`
# [+][PASS][Case 36/37] in=`(@Some true)`
# [+][PASS][Case 37/37] in=`(@Some false)`
# [=] 37/37 passed, 0 failed
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
        Option::None; {idx=2}
        Str(`Hello`); {idx=3,hash=274763}
        Str(`World`); {idx=4,hash=60723}

__entry:
        LOADG 3; Str(`Hello`)
        STORE 1
        LOADG 4; Str(`World`)
        STORE 2
        ARGS 2
        BUILTIN 166
```

Or of course the benchmark example:

```asm
__globals:
        False; {idx=0}
        True; {idx=1}
        Option::None; {idx=2}
        Double(2.5); {idx=3}

__entry:
; comparer::{args=2,size=20}
__0x000000[00B2]:
        JMP 20
        LOAD 1
        VAR 44
        LOAD 2
        VAR 229
        LOADV 44
        STORE 1
        LOADV 229
        EQ 1
        ASSERT
        LEAVE


; inc::{args=1,size=18}
__0x000016[0047]:
        JMP 40
        LOAD 1
        VAR 44
        LOADV 44
        STORE 1
        LOADV 44
        STORE 2
        ARGS 2
        CALL 0; <comparer> $2
        LEAVE

        LOADG 3; Double(2.5)
        STORE 1
        CALL 22; <inc> $1
```

The disassembler attempts to display as much information as possible:

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
# [    0.0240ms] io::IO_read_file_to_string: mmaped input of size=2500090B
# [    0.0050ms] mem::init: Allocated memory block of size=153092096B
# [   28.6080ms] lexer::Lexer_all: lexed tokens count=1000033
# [   16.7020ms] parser::Parser_next created AST with node_count=250003
# [    9.4510ms] cc::cc: Flattened AST to byte code/global pool length=1500048/4
# [   35.7510ms] vm::Vm_run: executed byte code
# [    0.4220ms] mem::Allocator::destroy: Deallocated memory space
# [    0.0000ms] vm::Vm_destroy: teared vm down
# [    0.0000ms] munmap: unmapped input

# or hyperfine
$ make release
$ hyperfine "./purple_garden examples/bench.garden"
# Benchmark 1: ./build/purple_garden examples/bench.garden
#   Time (mean ± σ):      80.8 ms ±   4.6 ms    [User: 71.9 ms, System: 7.6 ms]
#   Range (min … max):    74.7 ms … 101.6 ms    34 runs
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
  - [x] optionals (support in backend - compiler, vm)
  - [ ] objects
- [ ] language constructs
  - [x] variables
  - [ ] if
  - [ ] match
  - [ ] loops
  - [x] functions
  - [ ] pattern matching
- [x] builtins
  - [x] println
  - [x] print
  - [x] len
  - [x] type
  - [x] assert (interpreter intrinsic)

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
- [x] `vm`: preallocate 128 frames for even faster scope interaction (recursion)
- [x] `vm`: `EQ` fastpath for checking if lhs and rhs point to the same memory region
- [ ] `jit`: native compile functions before enterning the runtime, enabled via `+aot-functions`
- [ ] `cc`: multiple string concatinations should use a shared buffer and only allocate on string usage
- [x] `cc`: deadcode elimination for empty functions and their callsites
- [ ] `vm`: trail call optimisation
- [ ] `vm`: merge smaller bytecode ops often used together into new ops
    - [x] `vm`: merge `LOAD` and `PUSH` consecutively into `PUSHG`
- [ ] `vm`: lock I/O for the whole program execution for faster performance via `+lock-io`
- [ ] `cc`: cache bytecode and global pool to omit frontend, disable via `+no-cache`
- [ ] `gc`: mark and sweep garbage collection via `+gc-marksweep`
- [ ] `gc`: generational garbage collection via `+gc-gen`
- [ ] `gc`: reference counting via `+gc-rc`
- [x] `gc`: allow for bump/block allocator with `+block-allocator`
