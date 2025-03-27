## Bytecode and virtual machine design

This is a document to group my ideas about the bytecode design for the purple
garden compiler and virtual machine. All ideas inside of this can and most
likely will change.

## If and more

> I dont really like `if`/`else` etc, match is more fun - but a singular if
> could be useful? similar to ternary operator in procedural languages

```racket
(+ 1 1)
(if true (+ 1 1))
(* 1 1)
```

```asm
; (+ 1 1)
    LOAD 0  ; load 1 from constants
    STORE 1 ; move to r1
    LOAD 1  ; load 1 from constants
    ADD 1   ; add r0 and r1

; (if 1 (+ 1 1))
    LOAD 2  ; load true from constants
    N_JMP 8 ; jump bytecode indexes (op and arguments together) ahead if r0 false

; (+ 1 1)
    LOAD 3  ; load 1 from constants
    STORE 1
    LOAD 4
    ADD 1

; (N_JUMP) jumps here
; (* 1 1)
    LOAD 5
    STORE 1
    LOAD 6
    MUL 1 
```

## Match

```racket
(match k
    (true) "true"
    (false) "false")
```

```asm
    VAR 0   ; load value from variable with name at globals 0 into r0 'k'
    STORE 1 ; move to r1

; branch (true) "true"
    LOAD 1  ; load constant value 'true' into r0
    EQ 1    ; compare r0 with r1, r0 is true/false depending on value
    N_JMP 2 ; jump one instruction and its argument ahead if r0 false
    LOAD 2  ; load "true" from globals

; branch (false) "false"
    LOAD 3  ; load constant 'false' from globals
    EQ 1    ; compare r0 with r1, r0 is true/false depending on value
    N_JMP 2 ; jump one instruction ahead if r0 is false
    LOAD 4  ; load "false" from globals into r0
```

## Lambdas

```racket
((lambda (num) (* num num)) 5)
```

```asm
; lambda def
    VAR 1   ; load value for 'num' into r0
    STORE 1 ; move value to r1
    VAR 1   ; load second value
    MUL 1   ; multiply r0 and r1
    RETURN  ; ends lambda scope, return value in r0

; lambda call
    LOAD 0  ; load 5 from constants
    STORE 1 ; move to r1
    LOAD 1  ; r0 contains variable name 'num'
    SET 1   ; set variable name in r0 to value in r1
    CALL 0  ; jump to lambda def, requires a table in the compiler to keep track
            ; of function definitions, how will i implement functions as values?
```
