# purple_garden

purple_garden is a lean scripting language, designed and implemented with a
focus on performance via strategies like aggressive compile time optimisations,
just in time compilation for runtime hotspots, fine grained control over memory
and gc, while allowing to disable the gc and stdlib fully.

```python
fn greeting :: greetee {
    std::println("hello world to:" greetee)
} 
greeting(std::env::get("USER")) # hello world to: $USER

fn tuplify :: v {
    [type(v) len(v)]
} 
std::println(tuplify("hello world")) # [str, 11]
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
make run

# Args{
#         block_allocator: 0,
#         aot_functions: false,
#         disassemble: false,
#         memory_usage: false,
#         run: "",
#         verbose: false,
#         stats: false,
#         version: false,
#         gc_max: 1638400,
#         gc_size: 51200,
#         gc_limit: 70.000000,
#         no_gc: false,
#         no_std: false,
#         no_env: false,
# }
# # vim: filetype=python
# std::println("Hello" std::env::get("USER")) # Hello $USER
# vmnew: 16.00KB of 40.25KB used (39.751553%)
# N_PATH[T_STD](
#  N_CALL[println](
#   N_ATOM[T_STRING][Hello]{.hash=7201466553693376363},
#   N_PATH[T_STD](
#    N_IDENT[T_IDENT][env]{.hash=14046746036577462228},
#    N_CALL[get](
#     N_ATOM[T_STRING][USER]{.hash=10484170954014828594}
#    )
#   )
#  )
# )
# [cc] Found std leaf function `println`: 0x419780
# reserving r1
# [cc] Found std leaf function `get`: 0x41a580
# reserving r2
# freeing r2
# reserving r2
# freeing r2
# freeing r1
# __globals:
#         false; {idx=0}
#         true; {idx=1}
#         Option::None; {idx=2}
#         str::"Hello"; {idx=3,hash=7201466553693376363}
#         str::"USER"; {idx=4,hash=10484170954014828594}
# 
# __start:
#         LOADG 3; str::"Hello"
#         STORE 1
#         LOADG 4; str::"USER"
#         STORE 2
#         ARGS 129 ; count=1,offset=1
#         SYS 0
#         STORE 2
#         ARGS 256 ; count=2,offset=0
#         SYS 1
# cc  : 32.12KB of 40.25KB used (79.794255%)
# [VM][00000|00001] LOADG     =0000000003{ Option::None Option::None }
# [VM][00002|00003] STORE     =0000000001{ str::"Hello" Option::None }
# [VM][00004|00005] LOADG     =0000000004{ str::"Hello" str::"Hello" }
# [VM][00006|00007] STORE     =0000000002{ str::"USER" str::"Hello" }
# [VM][00008|00009] ARGS      =0000000129{ str::"USER" str::"Hello" }
# [VM][00010|00011] SYS       =0000000000{ str::"USER" str::"Hello" }
# [VM][00012|00013] STORE     =0000000002{ str::"teo" str::"Hello" }
# [VM][00014|00015] ARGS      =0000000256{ str::"teo" str::"Hello" }
# [VM][00016|00017] SYS       =0000000001{ str::"teo" str::"Hello" }
# Hello teo
# vm  : 0.00KB of 50.00KB used (0.000000%)
# | Opcode     | Compiled %               | Executed %               |
# | ---------- | ------------------------ | ------------------------ |
# | STORE      | 3               (33.33%) | 3               (33.33%) |
# | ARGS       | 2               (22.22%) | 2               (22.22%) |
# | SYS        | 2               (22.22%) | 2               (22.22%) |
# | LOADG      | 2               (22.22%) | 2               (22.22%) |
# | ========== | ======================== | ======================== |
# | ::<>       | 9               (99.99%) | 9               (99.99%) |

# provide a custom file to execute
make PG=examples/ops.garden
```

### Release builds

> produces a ./purple_garden binary with versioning information and optimisations

```sh
$ make release
$ ./purple_garden
# error: Missing a file? try `+h/+help`
$ ./purple_garden +h
# usage ./build/purple_garden: [ +b/+block_allocator <0>] [ +a/+aot_functions]
#                                    [ +d/+disassemble] [ +m/+memory_usage]
#                                    [ +r/+run <``>] [ +V/+verbose]
#                                    [ +s/+stats] [ +v/+version]
#                                    [ +gc_max <1638400>] [ +gc_size <51200>]
#                                    [ +gc_limit <70>] [ +no_gc]
#                                    [ +no_std] [ +no_env]
#                                    <file.garden>
# 
# Option:
#           +b/+block_allocator <0>
#                 use block allocator with size instead of garbage collection
#           +a/+aot_functions
#                 compile all functions to machine code
#           +d/+disassemble
#                 readable bytecode representation with labels, globals and comments
#           +m/+memory_usage
#                 display the memory usage of parsing, compilation and the virtual machine
#           +r/+run <``>
#                 executes the argument as if given inside a file
#           +V/+verbose
#                 verbose logs
#           +s/+stats
#                 show statistics
#           +v/+version
#                 display version information
#           +gc_max <1638400>
#                 set hard max gc space in bytes, default is GC_MIN_HEAP*64
#           +gc_size <51200>
#                 define gc heap size in bytes
#           +gc_limit <70>
#                 instruct memory usage amount for gc to start collecting, in percent (5-99%)
#           +no_gc
#                 disable garbage collection
#           +no_std
#                 limit the standard library to std::len
#           +no_env
#                 skip importing of all env variables
#           +h/+help
#                 help page and usage
# Examples:
#         ./build/purple_garden +b 0 +a \
#                                     +d +m \
#                                     +r "" +V \
#                                     +s +v \
#                                     + 1638400 + 51200 \
#                                     + 0 + \
#                                     + +
# 
#         ./build/purple_garden +block_allocator 0 +aot_functions \
#                                     +disassemble +memory_usage \
#                                     +run "" +verbose \
#                                     +stats +version \
#                                     +gc_max 1638400 +gc_size 51200 \
#                                     +gc_limit 0 +no_gc \
#                                     +no_std +no_env
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
# Hello teo
```

Results in `Hello teo` and of course bytecode disassembly:

```asm
__globals:
        false; {idx=0}
        true; {idx=1}
        Option::None; {idx=2}
        str::"Hello"; {idx=3,hash=7201466553693376363}
        str::"USER"; {idx=4,hash=10484170954014828594}

__start:
        LOADG 3; str::"Hello"
        STORE 1
        LOADG 4; str::"USER"
        STORE 2
        ARGS 129 ; count=1,offset=1
        SYS 0
        STORE 2
        ARGS 256 ; count=2,offset=0
        SYS 1
```

The disassembler attempts to display as much information as possible:

- elements of the global pool, their pool index and their hashes
- readable bytecode names: `LOAD` and `BUILTIN` instead of `0` and `6`
- global pool values for certain bytecode operators: ```global=Str(`Hello World`)```
- names for builtin calls: `builtin=println`
- labels for function definitions `<function>:` and branching `if:`, `then:`, `match:`, `default:`
- names for arguments, functions and variabels

### Benchmarks

For benchmarking, remember to create a large sample size via the purple garden
source code:

```python
# benchmarks/bin.garden
fn bin_step :: n k i r {
    match {
        i = k { std::Some(r) }
        {
            bin_step(n k i+1 r * (n-i)/(i+1))
        }
    }
}

fn bin :: n k {
    match {
        k < 0 { std::None() }
        k = 0 { std::Some(1) }
        k = n { std::Some(1) }
        {
            var kk = match {
                n - k < k { n - k }
                { k }
            }
            bin_step(n kk 0 1)
        }
    }
}

std::assert(bin(1 1) = 1)
std::assert(bin(1 0) = 1)
std::assert(bin(6 3) = 20)
std::assert(bin(6 5) = 6)
std::assert(bin(10 5) = 252)
std::assert(bin(20 15) = 15504)
std::assert(bin(49 6) = 13983816)
```

Running the whole thing with `make bench`, the time took for each stage is
notated between `[` and `]`.

```sh
# built in time measurements
$ make bench PG=benchmarks/bin.garden
# [    0.0000ms] main::Args_parse: Parsed arguments
# [    0.0070ms] io::IO_read_file_to_string: mmaped input of size=647B
# [    0.0050ms] mem::init: Allocated memory block of size=41216B
# [    0.0730ms] cc::cc: Flattened AST to byte code/global pool length=376/19 (1504B/304B)
# [    0.0090ms] vm::Vm_run: executed byte code
# [    0.0070ms] mem::Allocator::destroy: Deallocated memory space
# [    0.0000ms] munmap: unmapped input

# or hyperfine
$ make release
$ hyperfine "./build/purple_garden benchmarks/bin.garden" -N -w10
# Benchmark 1: ./build/purple_garden benchmarks/bin.garden
#   Time (mean ± σ):     610.4 µs ±  42.7 µs    [User: 288.9 µs, System: 245.5 µs]
#   Range (min … max):   549.0 µs … 1074.5 µs    4807 runs
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
- [x] `cc`: replace `std::<pkg>::<fn>` calls with indexes into `builtin::BUILTIN_MAP` to move function lookup from runtime to compile time
- [x] `cc`: compute `std::<pkg>::<fn>` indexes for identifiers via hash compare with precomputed builtin hashes
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
- [x] `gc`: hybrid garbage collection strategy combining: bump allocation, mark-and-sweep, and semi-space copying
- [x] `gc`: allow for bump/block allocator with `+block-allocator`
