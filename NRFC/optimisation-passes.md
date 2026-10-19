# Immediate representation optimisation passes

## Optimisation pipeline overview

```text
frontend -> IR -> opt::ir         per function
               -> dce_functions   whole program, only for programs run from the interpeters binary
               -> bc::Cc          bytecode, functions compiled natively by the jit
               -> opt::bc         peephole over the bytecode, then Cc::compact_nops
```

Optimisations in `opt::ir` make use of the immediate representation having the
following properties:

- functions are defined before they are called
- all instructions adhere to the static single assignment form and are in the form `dst:type = instr args`
- all controlflow is modelled via blocks with parameters and a terminator

## IR pipeline

The order is relevant, since some optimisations produce patterns later passes then recognize and may optimise again.

| name                     | description                                                           | order                                                                          |
| ------------------------ | --------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `const_fold_const_calls` | replaces calls to constant functions with their constant              | first, so `const_fold` folds the constants it inserts                          |
| `inline`                 | replaces calls to small single block functions with their body        | before `const_fold`, so folding sees through inlined bodies                    |
| `const_fold`             | folds instructions whose operands are all constants                   | before `imm_fold`, which would split constant expressions into immediates      |
| `const_fold_syscalls`    | folds `pure` builtins with constant arguments                         | after `const_fold`, for its constants                                          |
| `arith_simplify`         | algebraic identities, `x * 2` to `x + x`, `x % 2^k == 0` to `x & ...` | after `const_fold`, before `imm_fold` turns `x * 2` into an immediate multiply |
| `imm_fold`               | folds single use i32 constants into `BinImm`                          |                                                                                |
| `branch_cmp`             | fuses a compare used only by a branch into `BranchCmpImm`/`BranchCmp` |                                                                                |
| `switch_fold`            | folds a chain of equality branches on one subject into a `Switch`     | after `branch_cmp`, which produces the chains                                  |
| `ret_inline`             | turns `jmp join(v)` to a `ret`-only join into `ret v`                 | before `indirect_jump` and `tailcall`                                          |
| `switch_lookup`          | turns a `Switch` whose arms only return constants into a `Lookup`     | after `ret_inline`, which gives every arm its own `ret`                        |
| `indirect_jump`          | branches directly to the target of a block holding only a `jmp`       |                                                                                |
| `tailcall`               | turns a call whose value is returned into `Tail`                      | late, so inlining and folding see plain calls; never in the entry              |
| `addrof_fold`            | folds single use `AddrOf` chains into the offset of their users       |                                                                                |
| `load_store_fold`        | block local `Load`/`Store` rewrites                                   |                                                                                |
| `dce`                    | removes definitions nothing uses, to a fixed point                    | last, so the backends and their register allocators see less                   |

### `const_fold_const_calls`

A constant function is a single block of `LoadConst` returning one of them:

```text
// PI
fn f1() -> Double {
    b0():
        %v0:Double = 3.1415
        ret %v0
}

%v1:Double = Call f1()     =>     %v1:Double = 3.1415
```

### `inline`

Inlines a callee that is a single live block ending in `ret %vX`, holds no
`Call`, `Sys` or `Lookup` and at most `MAX_INSTRUCTIONS` instructions. 

Whether a function qualifies is decided once after its pipeline ran, call sites
look this up via `inlinable[id]`. Callee params become call arguments, all
other callee id `n` become `base + n`, with `base` past the highest id of the
caller. The call dst is aliased to the renamed return value:

```text
// is_even
fn f1(%v0:Int) -> Bool {
    b0(%v0):
        %v2:Int = IAnd %v0, 1
        %v4:Bool = IEq %v2, 0
        ret %v4
}

    %v4:Bool = Call f1(%v0)                %v17:Int = IAnd %v0, 1
    br %v4, b3(..), b4(..)        =>       %v18:Bool = IEq %v17, 0
                                           br %v18, b3(..), b4(..)
```

Calls in tail position are inlined too, since the `tailcall` pass runs later.

### `const_fold` and `const_fold_syscalls`

```text
%v0:Int = 25
%v1:Int = 12          =>     %v2:Int = 37
%v2:Int = IAdd %v0, %v1
```

> syscalls in purple garden refer to functions of its standard library

`const_fold_syscalls` does the same for builtins marked side effect free via `pure` through their `eval`
companion generated by `pg_fn(pure)`, see below:

```rust
#[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
pub mod math {
    /// Returns the non-negative square root of `n`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "math"
    ///
    /// math.sqrt(2.0)
    /// ```
    #[pg_fn(pure)]
    pub fn sqrt(n: f64) -> f64 {
        n.sqrt()
    }
}
```

This allows the optimiser to call these at optimisation time and replacing the
call with loading the returning value, for instance consider this garden script:

```garden
import ("math" "io")
io.println(math.sqrt(3.1415))
```

With optimisations disabled (`-O0`):

```text
// entry
fn f0() -> Void {
        b0():
                %v0:Double = 3.1415
                %v1:Double = Sys math.sqrt(%v0)
                %v2:Void = Sys io.println_double(%v1)
                ret %v2
}
```

Vs with optimisations enabled:

```text
// entry
fn f0() -> Void {
        b0():
                %v1:Double = 1.772427713617681
                %v2:Void = Sys io.println_double(%v1)
                ret %v2
}
```


### `arith_simplify`

Rewrites arithmetics to equivalent but less costly operations, a non
comprehensive list:

- `x * 2` to `x + x` for Int and Double
- `x * 0` to `0`, `x - x` to `0`, `x % 1` to `0`, only for Int
- `x == x` to `true`, `x < x` and `x > x` to `false`
- `x % 2^k == 0` to `x & (2^k - 1) == 0`, only for Int if rems only use
  is the test and the divisor constant has no other use

### `imm_fold`

A single use constant fitting in i32 becomes the immediate of the binary operation:

```text
%v1:Int = 1
%v2:Int = IAdd %v0, %v1     =>     %v2:Int = IAdd %v0, 1
```

### `branch_cmp`

Removes materialisation of a boolean in a value if its used to branch
immediately:

```text
%v3:Bool = IEq %v0, 1
br %v3, b1(..), b2(..)      =>     br_imm IEq %v0, 1, b1(..), b2(..)
```

### `switch_fold` and `switch_lookup`

- `switch_fold` turns a list of `br_imm IEq`/`br_cmp SEq` based on a single
subject into a `Switch`. This is then lowered into a jump table in the
backends. 

- `switch_lookup` turns a `Switch` whose arms only return constants into one
`Lookup`:

```text
switch %v0                       %v9 = Lookup %v0 [`a` -> 1, `b` -> 2], 0
    `a` -> b1    ret 1     =>     ret %v9
    `b` -> b2    ret 2
    _   -> b3    ret 0
```

### `ret_inline` and `indirect_jump`

`ret_inline` gives every arm of an `if`/`match` its own `ret` instead of the
indirection of jumping to a join block containing only `ret`, the join becomes
a tombstone once every predecessor is rewritten. `indirect_jump` points a
branch at the target of a block that only holds a `jmp`.

### `tailcall`

A call whose value is returned right away, directly (`call; ret`) or through a
`ret`-only join (`call; jmp join`) is replaced with `tail f(args)`. Self tail
calls become loops, tail calls to other native functions `jmp` in the jit.

### `addrof_fold` and `load_store_fold`

`addrof_fold` folds a chain of single use `AddrOf`s into the base and offset of
the `Load`, `Store` or `AddrOf` using it, so nested record access becomes one
memory operand. `load_store_fold` merges multiple loads and stores resolving to
the same logical operation

### `dce`

Removes definitions without uses until nothing changes, removing one can make
the definitions it read dead too. Keeps `Call`, `Assert` and non `pure` `Sys`.

## Whole program: `dce_functions`

Marks every function the entry can't reach as `Func::unused`, so backends dont
have to waste time lowering them. THis is CLI only, since embedders may call
any function

## Bytecode peephole: `opt::bc`

become `Nop` so jump targets stay valid, `Cc::compact_nops` removes them after:

- `self_move`: removes `mov rX, rX`
- `mov_merge`: `mov r8, r0; mov r2, r8` to `mov r2, r0`, a fallback for moves
  the copy propagation misses between blocks
- `jmp_next`: removes a `jmp` to the next instruction
