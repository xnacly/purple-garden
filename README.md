# purple_garden

> purple_garden is a minimal lisp I am attempting to make as fast possible.

```racket
(let greeting
    (lambda (greetee) (+ "hello world to: " greetee)))
(println (greeting "user"))
; prints `hello world to: user`
```


## Run

Currently there isn't much implemented, but you can test purple_garden as follows:

```sh
# by default purple_garden fills $PG to be ./examples/hello-world.pg
make

# results in:
# 
#   N_LIST(
#    N_LIST(
#     N_IDENT[T_IDENT]('println'),
#     N_ATOM[T_STRING]('Hello World')
#    )
#   )

# provide a custom file to execute
make PG=examples/ops.pg
```
