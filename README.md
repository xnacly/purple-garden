# purple_garden

purple_garden is a lean scripting language, designed and implemented with a
focus on performance via strategies like aggressive compile time optimisations,
just in time compilation for runtime hotspots, fine grained control over memory
and gc, while allowing to disable the gc and stdlib fully.

```python
fn greeting :: greetee {
    println("hello world to:" greetee)
} 
greeting("teo") # hello world to: user

fn tuplify :: v {
    [type(v) len(v)]
} 
println(tuplify("hello world")) # [str, 11]
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

# ./build/purple_garden_debug ./examples/hello-world.garden
# #vim: filetype=python
# 
# #fmt/println is a predefined function responsible for writing to
# #stdout
# println("Hello World")
# vmnew: 8.03KB of 24.25KB used (33.118557%)
# ->Parser_comparison#T_IDENT
#  ->Parser_expr#T_IDENT
#   ->Parser_term#T_IDENT
#    ->Parser_atom#T_IDENT
#     ->Parser_next#T_STRING
#      ->Parser_comparison#T_STRING
#       ->Parser_expr#T_STRING
#        ->Parser_term#T_STRING
#         ->Parser_atom#T_STRING
# N_CALL[println](
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
# __start:
#         LOADG 3; Str(`Hello World`)
#         STORE 1
#         ARGS 128 ; count=1,offset=0
#         BUILTIN 326
# cc  : 10.77KB of 24.25KB used (44.426546%)
# [VM][000000|00001] LOADG     =000003{ Option/None Option/None }
# [VM][000002|00003] STORE     =000001{ Str(`Hello World`) Option/None }
# [VM][000004|00005] ARGS      =000128{ Str(`Hello World`) Str(`Hello World`) }
# [VM][000006|00007] BUILTIN   =000326{ Str(`Hello World`) Str(`Hello World`) }
# Hello World
# vm  : 4.02KB of 50.00KB used (8.046875%)
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
#$ ./purple_garden +h
#usage ./build/purple_garden_debug: [ +b / +block_allocator <0>] [ +a / +aot_functions]
#                                   [ +d / +disassemble] [ +m / +memory_usage]
#                                   [ +r / +run <``>] [ +V / +verbose]
#                                   [ +s / +stats] [ +v / +version]
#                                   [ +gc_max <1638400>] [ +gc_size <51200>]
#                                   [ +gc_limit <70>]
#                                   [ +h / +help] <file.garden>
#
#Option:
#          +b / +block_allocator <0>
#                use block allocator with size instead of garbage collection
#
#          +a / +aot_functions
#                compile all functions to machine code
#
#          +d / +disassemble
#                readable bytecode representation with labels, globals and comments
#
#          +m / +memory_usage
#                display the memory usage of parsing, compilation and the virtual machine
#
#          +r / +run <``>
#                executes the argument as if given inside a file
#
#          +V / +verbose
#                verbose logs
#
#          +s / +stats
#                show statistics
#
#          +v / +version
#                display version information
#
#          +gc_max <1638400>
#                set hard max gc space in bytes, default is GC_MIN_HEAP*64
#
#          +gc_size <51200>
#                define gc heap size in bytes
#
#          +gc_limit <70>
#                instruct memory usage amount for gc to start collecting, in percent (5-99%)
#
#          +h / +help
#                help page and usage
#
#Examples:
#        ./build/purple_garden_debug +b 0 +a \
#                                    +d +m \
#                                    +r "" +V \
#                                    +s +v \
#                                    + 1638400 + 51200 \
#                                    + 0
#
#        ./build/purple_garden_debug +block_allocator 0 +aot_functions \
#                                    +disassemble +memory_usage \
#                                    +run "" +verbose \
#                                    +stats +version \
#                                    +gc_max 1638400 +gc_size 51200 \
#                                    +gc_limit 0
```

### Running tests

```sh
$ make test
```


Tests are located in `tests/test.c` and a test is declared via the `CASE` macro:

```c
      CASE(3.1415, VAL(.type = V_DOUBLE, .floating = 3.1415)),
      CASE(0.1415, VAL(.type = V_DOUBLE, .floating = 0.1415)),
      CASE("string", VAL(.type = V_STR, .string = &STRING("string"))),
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

__start:
; comparer::{args=2,size=26}
__0x000000[02F2]:
        JMP 26
        LOAD 1
        VAR 140
        LOAD 2
        VAR 165
        LOADV 140
        STORE 1
        LOADV 165
        EQ 1
        STORE 1
        ARGS 128 ; count=1,offset=0
        BUILTIN 603
        LEAVE


; inc::{args=1,size=20}
__0x00001A[0087]:
        JMP 46
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
        CALL 26; <inc> $1
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
fn comparer :: a b { assert(a = b) }
fn inc :: a { comparer(a a) }
inc(2.5)
inc(2.5)
inc(2.5)
; [...]
```

Running the whole thing with `make bench`, the time took for each stage is
notated between `[` and `]`.

```sh
# built in time measurements
$ make bench PG=examples/bench.garden
# [    0.0000ms] main::Args_parse: Parsed arguments
# [    0.0080ms] io::IO_read_file_to_string: mmaped input of size=2250076B
# [    0.0020ms] mem::init: Allocated memory block of size=24832B
# [   34.6260ms] cc::cc: Flattened AST to byte code/global pool length=2000054/4 (8000216B/64B)
# [   16.5160ms] vm::Vm_run: executed byte code
# [    1.5930ms] mem::Allocator::destroy: Deallocated memory space
# [    0.0010ms] vm::Vm_destroy: teared vm down
# [    0.0000ms] munmap: unmapped input

# or hyperfine
$ make release
$ hyperfine "./build/purple_garden examples/bench.garden"
# Benchmark 1: ./build/purple_garden examples/bench.garden
#   Time (mean ± σ):      53.0 ms ±   0.7 ms    [User: 37.1 ms, System: 15.5 ms]
#   Range (min … max):    52.2 ms …  54.9 ms    53 runs
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

- [x] data types
  - [x] numbers
  - [x] strings
  - [x] booleans
  - [x] lists
  - [x] optionals (support in backend - compiler, vm)
  - [x] objects
- [ ] language constructs
  - [x] variables
  - [x] match
  - [x] functions
  - [x] standard library
  - [ ] iteration
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
- [ ] `gc`: mark and sweep garbage collection via `+gc:marksweep`
- [ ] `gc`: generational garbage collection via `+gc:gen`
- [ ] `gc`: reference counting via `+gc:rc`
- [x] `gc`: allow for bump/block allocator with `+block-allocator`
