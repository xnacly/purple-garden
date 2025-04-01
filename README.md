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
