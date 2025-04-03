# purple_garden

> purple_garden is a minimal lisp I am attempting to make as fast possible.

```racket
(@function greeting (greetee)
    (+ "hello world to: " greetee))
(@println (greeting "user"))
; prints `hello world to: user`
```

## Local Setup

### Running

```sh
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
#         Str(`Hello World`); [0]
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

```bash
# produces a ./purple_garden binary with versioning information and
# optimisations
make release
./purple_garden
# usage: purple_garden [--disassemble] [--version] [--help] <file.garden>
# purple-garden: ASSERT(a.filename != NULL): `Wanted a filename as an argument, not enough arguments` failed at ./main.c, line 106
./purple_garden --help
# purple_garden: pre-alpha-a587526
# 
# usage: purple_garden [--disassemble] [--version] [--help] <file.garden>
# 
# Options:
#         --disassemble     readable bytecode representation with labels, globals and comments
#         --version         display version information
#         --help            extended usage information
```

### Disassembling bytecode

```bash
./purple_garden --disassemble <file.garden>
```

For readable bytecode representation with labels, globals and comments.

> `--disassemble` is enabled by default when in debug builds via `-DDEBUG=1`

```bash
$ ./purple_garden --disassemble examples/hello-world.garden
# [...] omitted - see below
# Hello World
```

Results in `Hello World` and of course bytecode disassembly:

```asm
; vim: filetype=asm
; Vm {global=1/128, bytecode=4/1024}
globals:
        Str(`Hello World`); [0]

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
- bytecode operator and argument values and indexes: `[op=6,arg=0] at (0/1)`
- readable bytecode names: `LOAD` and `BUILTIN` instead of `0` and `6`
- global pool values for certain bytecode operators: ```global=Str(`Hello World`)```
- names for builtin calls: `builtin=@println`
- labels for function definitions `<function>:` and branching `if:`, `then:`, `match:`, `default:`

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
- [ ] `cc`: intern atoms to deduplicate `Vm.globals`
- [ ] `cc`: bump allocator for globals and bytecode (separate or shared?)
- [ ] `cc`: multiple string concatinations should use a shared buffer and only allocate on string usage
- [ ] `vm`: trail call optimisation
- [ ] `vm`: merge smaller bytecode ops often used together into new ops
- [ ] `gc`: mark and sweep garbage collection, allow for bump/block allocator
      with `--alloc-block` (useful for small scripts -> less time in gc)
