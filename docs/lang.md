# Purple garden language reference

> Since purple garden (pg, or: purple) is in active development, this document
> may be outdated.

## Types

Purple garden is dynamically typed and type compatibility is checked at
runtime.

- Strings: `"this is a string"`
- Numbers: `3.1415`, `.00001`, `5e+10`, `5e-10`
- Booleans: `true`, `false`
- Optionals: `(@Some "inner")`, `(@None)`
- Records: `@{ name="User" age=22 job={ name="Developer" } }`, access: `@{}.job.name`
- Lists: `(true .1415 "hello")`, access: `().0`

> Records and lists can be iterated over, everything is supported in pattern
> matching

## Basic operators

- Arithmetics:
    ```racket
    (+ 1 .1415)
    (- 2 .1415)
    (* 12 1920)
    (/ 8192)
    ```
- Equality: `(= true false)`, `(!= true false)`
- Relationship: `(< 1 2)`, `(> 1 2)`, `(>= 1 2)`, ``(<= 1 2)``
- Booleans: `(| true false)`, `(& true false)`, `(! true false)`


## Functions

Definitions:

```racket
@function greeting (greetee) { (+ "hello world to: " greetee) }
@function square (n) { (* n n) }
```

Calls:

```racket
(square 3.1415)
(greeting "xnacly")
```

## Builtins

Syntax constructs and builtins are prefixed with `@`

```racket
(@type "lalilu") ; "string"
(@type 3.1415) ; "number"

(@println "hello" "world")
(@print "yes")

(@len "12345") ; 5
(@len ()) ; 0
```

## Variables

```racket
@let str "this is a string"
@let pi 3.1415
@let tritratrue true
@let option_some (@Some "inner")
@let option_none (@None)
@let user @{ 
    name "User" 
    age 22 
    job { name "Developer" }
}
@let user_job_name user.job.name
@let arr_or_list (true .1415 "hello")
@let arr_or_list0 arr_or_list.0
```

## Control flow

### If

```racket
@let n 5 
@if (!= n 2) { (@println "n!=2") }
```

### For

```racket
@let names ("tom" "tim" "tam" "tel")
@for name names {
    (@println name)
}
```

## Pattern matching

Via inline:

```racket
(:0..9 5) ; returns true
@let ident_pattern :a..z A..Z
(ident_pattern b) ; true
(ident_pattern B) ; true
(ident_pattern 3) ; false

(:(_ 5) (2 5)) ; true
(:(_ 5) (2 2)) ; false
(:(_ 5) (2 5 1)) ; false
(:(_ 5 _) (2 5 1)) ; true

(:@{name "xnacly"} @{name "xnacly"}) ; true
@let user @{name "xnacly" age 25}
; true and puts age=25 and name into scope
@if (:@{name _ age 25} user) { (@println name "is 25 years old") }

@let words "word1 word2 word3 word4"
@for capture (:"(\w\+)" words) { (@println capture) }
```

Via `match`:

```racket
@let user @{name "xnacly" age 25}
@match {
    (:@{ name (:"x\w" }) { (@println name "starts with 'x'") }
    (:@{ age 20 }) { (@println name "is" 20) }
    (>= 18 user) { (@println user.name "is at least 18") }
    { (@println user.name "is not over 18") }
}
```
