# purple_garden

> purple_garden is a minimal lisp I am attempting to make as fast possible.

```racket
(@function greeting (greetee)
    (+ "hello world to: " greetee))
(@println (greeting "user"))
; prints `hello world to: user`
```

## Run

Currently there isn't much implemented, but you can test purple_garden as follows:

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
# ================= GLOB =================
# VM[glob1/1] String(`Hello World`)
# ================= VMOP =================
# VM[000000(000001)] OP_LOAD(0)
# VM[000002(000003)] OP_BUILTIN(0)
# Hello World
# ================= REGS =================
# VM[r0]: Option(None)
# VM[r1]: undefined
# VM[r2]: undefined


# provide a custom file to execute
make PG=examples/ops.garden
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
- [ ] `cc`: intern atoms to deduplicate `Vm.globals`
- [ ] `cc`: bump allocator for globals and bytecode (separate or shared?)
- [ ] `cc`: multiple string concatinations should use a shared buffer and only allocate on string usage
- [ ] `vm`: trail call optimisation
- [ ] `vm`: merge smaller bytecode ops often used together into new ops
- [ ] `gc`: mark and sweep garbage collection, allow for bump/block allocator
      with `--alloc-block` (useful for small scripts -> less time in gc)
