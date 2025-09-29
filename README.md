# purple_garden

purple_garden is a lean lisp, designed and implemented with a focus on
performance

```racket
(fn greeting [greetee] (println "hello world to:" greetee))
(greeting "teo") ; hello world to: user

(fn tuplify [v] [(type v) (len v)])
(println (tuplify "hello world")) ; [str, 11]
```

## Local Setup

> purple garden is a C project, so you will need a C compiler toolchain and
> make if you want to following along or you could use the flake :)

```sh
$ git clone git@github.com:xnacly/purple-garden.git
# or
$ git clone https://github.com/xnacly/purple-garden.git
```

```sh
# flake flake, if you want
nix develop

# by default purple_garden fills $PG to be ./examples/hello-world.garden
make

# ; vim: filetype=racket
# 
# ; @println is a predefined function responsible for writing to stdout
# ; builtins are specifically called via @<builtin>
# (println "Hello World")
# vmnew: 8.00KB of 25.00KB used (32.000000%)
# [T_DELIMITOR_LEFT]
# [T_BUILTIN][println]{.hash=8602815819212105030}
# [T_STRING][Hello World]{.hash=4420528118743043111}
# [T_DELIMITOR_RIGHT]
# N_BUILTIN[T_BUILTIN][println]{.hash=8602815819212105030}(
#  N_ATOM[T_STRING][Hello World]{.hash=4420528118743043111}
# )
# allocating r1
# freeing r1
# __globals:
#         False; {idx=0}
#         True; {idx=1}
#         Option/None; {idx=2}
#         Str(`Hello World`); {idx=3,hash=39}
# 
# __entry:
#         LOADG 3; Str(`Hello World`)
#         STORE 1
#         ARGS 128 ; count=1,offset=0
#         BUILTIN 326
# cc  : 11.27KB of 25.00KB used (45.093750%)
# [VM][000000|00001] LOADG     =000003{ Option/None Option/None }
# [VM][000002|00003] STORE     =000001{ Str(`Hello World`) Option/None }
# [VM][000004|00005] ARGS      =000128{ Str(`Hello World`) Str(`Hello World`) }
# [VM][000006|00007] BUILTIN   =000326{ Str(`Hello World`) Str(`Hello World`) }
# Hello World
# vm  : 4.23KB of 50.00KB used (8.468750%)
# | Opcode     | Compiled %               | Executed %               |
# | ---------- | ------------------------ | ------------------------ |
# | STORE      | 1               (25.00%) | 1               (25.00%) |
# | ARGS       | 1               (25.00%) | 1               (25.00%) |
# | BUILTIN    | 1               (25.00%) | 1               (25.00%) |
# | LOADG      | 1               (25.00%) | 1               (25.00%) |
# | ========== | ======================== | ======================== |
# | ::<>       | 4               (99.99%) | 4               (99.99%) |

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
# [+][PASS][Case 1/38] in=`(test_return 3.1415)`
# [+][PASS][Case 2/38] in=`(test_return 0.1415)`
# [+][PASS][Case 3/38] in=`(test_return "string")`
# [+][PASS][Case 4/38] in=`(test_return 'quoted)`
# [+][PASS][Case 5/38] in=`(test_return false)`
# [+][PASS][Case 6/38] in=`(test_return true)(test_return false)(test_return false)`
# [+][PASS][Case 7/38] in=`(test_return "hello")`
# [+][PASS][Case 8/38] in=`(+2 2)`
# [+][PASS][Case 9/38] in=`(-5 3)`
# [+][PASS][Case 10/38] in=`(*3 4)`
# [+][PASS][Case 11/38] in=`(/ 6 2)`
# [+][PASS][Case 12/38] in=`(+1(-2 1))`
# [+][PASS][Case 13/38] in=`(+2.0 2)`
# [+][PASS][Case 14/38] in=`(+2 2.0)`
# [+][PASS][Case 15/38] in=`(-5.0 3)`
# [+][PASS][Case 16/38] in=`(-5 3.0)`
# [+][PASS][Case 17/38] in=`(*3.0 4)`
# [+][PASS][Case 18/38] in=`(*3 4.0)`
# [+][PASS][Case 19/38] in=`(/ 6.0 2)`
# [+][PASS][Case 20/38] in=`(/ 6 2.0)`
# [+][PASS][Case 21/38] in=`(len "hello")`
# [+][PASS][Case 22/38] in=`(len "hello")(len "hello")`
# [+][PASS][Case 23/38] in=`(len "")`
# [+][PASS][Case 24/38] in=`(len "a")`
# [+][PASS][Case 25/38] in=`(= 1 1)`
# [+][PASS][Case 26/38] in=`(= "abc" "abc")`
# [+][PASS][Case 27/38] in=`(= 3.1415 3.1415)`
# [+][PASS][Case 28/38] in=`(= true true)`
# [+][PASS][Case 29/38] in=`(= true false)`
# [+][PASS][Case 30/38] in=`(= false false)`
# [+][PASS][Case 31/38] in=`(var name "user")(test_return name)`
# [+][PASS][Case 32/38] in=`(var age 25)(test_return age)`
# [+][PASS][Case 33/38] in=`(fn ret[arg] arg)(ret 25)`
# [+][PASS][Case 34/38] in=`(fn add25[arg](+arg 25))(add25 25)`
# [+][PASS][Case 35/38] in=`(assert true)`
# [+][PASS][Case 36/38] in=`(None)`
# [+][PASS][Case 37/38] in=`(Some true)`
# [+][PASS][Case 38/38] in=`(Some false)`
# [=] 38/38 passed, 0 failed
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
        Option/None; {idx=2}
        Str(`Hello World`); {idx=3,hash=39}

__entry:
        LOADG 3; Str(`Hello World`)
        STORE 1
        ARGS 128 ; count=1,offset=0
        BUILTIN 326
```

Or of course the benchmark example:

```asm
__globals:
        False; {idx=0}
        True; {idx=1}
        Option/None; {idx=2}
        Double(2.5); {idx=3}

__entry:
; comparer::{args=2,size=32}
__0x000000[02F2]:
        JMP 32
        LOAD 1
        VAR 140
        LOAD 2
        VAR 165
        LOADV 140
        STORE 1
        LOADV 165
        EQ 1
        JMPF 30
        LOADV 140
        STORE 1
        LOADV 165
        EQ 1
        ASSERT
        LEAVE


; inc::{args=1,size=20}
__0x000020[0087]:
        JMP 52
        LOAD 1
        VAR 140
        LOADV 140
        STORE 1
        LOADV 140
        STORE 2
        ARGS 256 ; count=2,offset=0
        CALL 0; <comparer> $2
        LEAVE

        LOADG 3; Double(2.5)
        STORE 1
        ARGS 128 ; count=1,offset=0
        CALL 32; <inc> $1
```

The disassembler attempts to display as much information as possible:

- elements of the global pool, their pool index and their hashes
- readable bytecode names: `LOAD` and `BUILTIN` instead of `0` and `6`
- global pool values for certain bytecode operators: ```global=Str(`Hello World`)```
- names for builtin calls: `builtin=println`
- labels for function definitions `<function>:` and branching `if:`, `then:`, `match:`, `default:`
- names for arguments, functions and variabels

### Benchmarks

For benchmarking, remember to create a large sample size via the purple garden source code:

```sh
$ wc -l examples/bench.garden
# 250003 examples/bench.garden
```

> This benchmark example is for optimizing tail calls, builtin dispatch and match performance:

```racket
(fn comparer [a b] (= a b))
(fn inc [a] (comparer a a))
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
# [    0.0070ms] io::IO_read_file_to_string: mmaped input of size=2500087B
# [    0.0020ms] mem::init: Allocated memory block of size=25600B
# [   53.0570ms] cc::cc: Flattened AST to byte code/global pool length=2000052/4 (8000208B/64B)
# [   18.7470ms] vm::Vm_run: executed byte code
# [    2.3230ms] mem::Allocator::destroy: Deallocated memory space
# [    0.0000ms] vm::Vm_destroy: teared vm down
# [    0.0000ms] munmap: unmapped inpu

# or hyperfine
$ make release
$ hyperfine "./purple_garden examples/bench.garden"
# Benchmark 1: ./build/purple_garden examples/bench.garden
#   Time (mean ± σ):      70.8 ms ±   2.2 ms    [User: 38.8 ms, System: 31.4 ms]
#   Range (min … max):    68.4 ms …  81.5 ms    41 runs
```

### Profiling

Using perf and [hotspot](https://github.com/KDAB/hotspot), you can get a
flamechart and other info:

```sh
$ make profile
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
  - [x] objects
- [ ] language constructs
  - [x] variables
  - [x] match
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
