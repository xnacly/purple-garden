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
# ================== IN ==================
# ; vim: filetype=racket
# 
# ; @println is a predefined function responsible for writing to stdout
# ; builtins are specifically called via @<builtin>
# (@println "Hello World")
# ================= TOKS =================
# [T_DELIMITOR_LEFT]
# [T_AT]
# [T_IDENT][println]
# [T_STRING][Hello World]
# [T_DELIMITOR_RIGHT]
# ================= TREE =================
# N_LIST(
#  N_LIST(
#   N_BUILTIN[T_IDENT][println],
#   N_ATOM[T_STRING][Hello World]
#  )
# )
# ================= DASM =================
# ; vim: filetype=asm
# ; Vm {global=1/128, bytecode=4/1024}
# globals:
#         Str(`Hello World`); {idx=0,hash=39}
# 
# entry:
#         ; [op=0,arg=0] at (0/1)
#         ; global=Str(`Hello World`)
#         LOAD 0
#         ; [op=6,arg=0] at (2/3)
#         ; builtin=@println
#         BUILTIN 0
# 
# ================= GLOB =================
# VM[glob1/1] Str(`Hello World`)
# ================= VMOP =================
# VM[000000(000001)] LOAD(0)
# VM[000002(000003)] BUILTIN(0)
# Hello World
# ================= REGS =================
# VM[r0]: Option(None)
# VM[r1]: undefined
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
#                      [-b | --block-allocator] [-a | --aot-functions] <file.garden>
# error: Missing a file? try `-h/--help`
$ ./purple_garden -h
# usage: purple_garden [-v | --version] [-h | --help] [-d | --disassemble]
#                      [-b | --block-allocator] [-a | --aot-functions] <file.garden>
# 
# options:
#         -v, --version         display version information
#         -h, --help            extended usage information
#         -d, --disassemble     readable bytecode representation with labels, globals and comments
#         -b, --block-allocator use block allocator instead of garbage collection
#         -a, --aot-functions   compile all functions to machine code
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
; Vm {global=1/128, bytecode=4/1024}
globals:
        Str(`Hello World`); {idx=0,hash=39}

entry:
        ; [op=0,arg=0] at (0/1)
        ; global=Str(`Hello World`)
        LOAD 0
        ; [op=6,arg=0] at (2/3)
        ; builtin=@println
        BUILTIN 0
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
$ make bench PG=examples/bench.garden
# [    0.0050ms] main::Args_parse: Parsed arguments
# [    0.0420ms] io::IO_read_file_to_string: mmaped input
# [   90.6160ms] parser::Parser_run: Transformed source to AST
# [   18.2240ms] cc::cc: Flattened AST to byte code
# [   34.5640ms] parser::Node_destroy: Deallocated AST Nodes
# [    2.1820ms] vm::Vm_run: Walked and executed byte code
# [    0.4700ms] vm::Vm_destroy: Deallocated global pool and bytecode list
```

### Profiling

Using perf and [hotspot](https://github.com/KDAB/hotspot), you can get a
flamechart and other info:

```sh
$ make release
$ perf record --call-graph dwarf ./purple_garden ./bench.garden
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
  - [ ] get

## Optimisations

- [x] `io`: `mmap` instead of reading the input file manually (6x-35x)(5/10k-250k loc)
- [x] `common`: turn `String` for `char*` abstraction into windows instead of allocating in the lexer (1.4x)(250k loc)
- [ ] `parser`: either attach parser directly to compiler or move to a bump allocator for `Node.children`
- [x] `cc`: replace `@<builtin>` calls with indexes into `builtin::BUILTIN_MAP` to move function lookup from runtime to compile time
- [ ] `cc`: intern atoms to deduplicate `Vm.globals`
- [ ] `cc`: bump allocator for globals and bytecode (separate or shared?)
- [ ] `cc`: multiple string concatinations should use a shared buffer and only allocate on string usage
- [ ] `vm`: trail call optimisation
- [ ] `vm`: merge smaller bytecode ops often used together into new ops
- [ ] `vm`: lock I/O for the whole program execution for faster performance via `--lock-io`
- [ ] `gc`: mark and sweep garbage collection, 
- [ ] `gc`: allow for bump/block allocator with `--alloc-block`
