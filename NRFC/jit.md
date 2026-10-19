# Tracking JIT progress

For current full scope of compilable purple garden script contents, see:
[examples/jitprogress.garden](../examples/jitprogress.garden):

## Calling convention

`Vm::r` is the interpreter virtual machine register file and is used at the
boundary between the interpreter and native code (either jit or builtin
functions (pgs stdlib implemenmted in rust)). Native code passes arguments and
results in hardware registers for performance reasons.

### Registers in jited code:

| register                     | role                                               |
| ---------------------------- | -------------------------------------------------- |
| `rbx`                        | `*mut Vm`, callee saved, set by `vm::enter_native` |
| `rsi, rdx, rcx, r8, r9, r10` | argument i of a native function, if not Double     |
| `xmm0..xmm5`                 | argument i of a native function, if Double         |
| `rax` / `xmm0`               | result, `xmm0` for Doubles                         |
| `rax`, `xmm15`               | scratch, not subject to regalloc, used as tmp      |
| `rbp, r12..r15`              | callee saved, pushed by native function using em   |

For instance arg i always uses slot i of the register list its type needs, `f(x:Int y:Double)` takes `x` in `rsi` and `y` in `xmm1`.

`rdi` is allocatable like any other caller saved register, it carries no
argument.

| register                    | `rdi` | `rsi`     | `rdx`     | `rcx`     | `r8`      | `r9`      | `r10`     |
| --------------------------- | ----- | --------- | --------- | --------- | --------- | --------- | --------- |
| builtin native entry (SysV) | vm    | arg 0     | arg 1     | arg 2     | arg 3     | arg 4     |           |
| native function             |       | arg 0     | arg 1     | arg 2     | arg 3     | arg 4     | arg 5     |
| loaded by `enter_native`    |       | `vm.r[0]` | `vm.r[1]` | `vm.r[2]` | `vm.r[3]` | `vm.r[4]` | `vm.r[5]` |

### Why native functions don't use SysV

Native functions differ from SysV:

- vm in `rbx` for native functions: it isnt clobbered by calls into Rust due to `rbx` being callee saved, `mov rdi, rbx` moves it into rdi for jit helpers and builtins
- argument slots are positional across both register files, arg i in gprI or xmmI
- args start at `rsi`, native function arg i sits where a
  builtin native entry takes arg i, after vm in `rdi`: thus arguments for a builtin dont need to be moved

### Interpreter -> native function

`enter_native` is used to transfer virtual machine state into hardware registers for the jit:

```rust
pub unsafe extern "C" fn enter_native(vm: *mut c_void, target: BuiltinFn){
    std::arch::naked_asm!(
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "sub rsp, 8",
        "lea rax, [rsp - 8]",
        "mov [rdi + {entry_rsp}], rax",
        "mov rbx, rdi",
        "mov r11, rsi",
        "mov rsi, [rdi]",
        "mov rdx, [rdi + 8]",
        "mov rcx, [rdi + 16]",
        "mov r8, [rdi + 24]",
        "mov r9, [rdi + 32]",
        "mov r10, [rdi + 40]",
        "movq xmm0, [rdi]",
        "movq xmm1, [rdi + 8]",
        "movq xmm2, [rdi + 16]",
        "movq xmm3, [rdi + 24]",
        "movq xmm4, [rdi + 32]",
        "movq xmm5, [rdi + 40]",
        "call r11",
        "mov [rbx], rax",
        "add rsp, 8",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
        "ret",
        entry_rsp = const crate::NATIVE_ENTRY_RSP_OFFSET,
    )
}
```

native function ptrs are stored in `vm.syscalls` (same as builtins), the
bytecode vm calls it via `Op::Native`. `returns_double` is derived from the callees
return type and requires `enter_native_double`, storing `xmm0` instead of
`rax`.

`enter_native` saves every callee saved register.

### Native function -> native function

- arguments are moved into place by one parallel move, together with copying
  values live across the call out of caller saved registers
- `call rel32`: native functions share one code arena, the displacement is
  patched once the function's place in the arena is known
- self calls are `call rel32` to the function's first byte
- tail calls are `jmp rel32` to the callee, or to a per callee epilogue
  `pop ...; jmp rel32 callee` if the caller has a frame
- a Double live across a call is kept in a callee saved gpr with `movq`, SysV
  has no callee saved xmm register
- a native function calling a function that stayed bytecode bails

### Native function -> builtin

`pg_fn` generates a native entry next to the wrapper: SysV with the vm first,
`f64` for Doubles and one `Value` word for everything else. A slot
(`Slot<"T">`) crosses as a `Value` word, a Double bound is moved between xmm
and gpr using `movq`.

### Traps

Native code may trap (division by zero, stack overflow, failed allocation,
failed assert) if so, it calls a `jit_trap_*` helper, recording the anomaly in
`vm.pending_trap`, leaving native "space" through `jit_abort`:

```text
jit_abort(vm):
  rbx = vm
  rsp = vm.native_entry_rsp    ; the slot of enter_native's return address
  ret                          ; continues after `call target` in enter_native
```

## Progress

| IR node        | variant                           | x86 | aarch64 |
| -------------- | --------------------------------- | --- | ------- |
| `Return`       |                                   | yes | no      |
| `Jump`         |                                   | yes | no      |
| `Branch`       |                                   | yes | no      |
| `BranchCmpImm` | `IEq`                             | yes | no      |
|                | other ops                         | no  | no      |
| `BranchCmp`    | `SEq`                             | yes | no      |
|                | other ops                         | no  | no      |
| `Switch`       |                                   | yes | no      |
| `Tail`         | self-recursive                    | yes | no      |
|                | native function                   | yes | no      |
|                | bytecode function                 | no  | no      |
| `Bin`          | `IAdd` / `ISub` / `IMul` / `IAnd` | yes | no      |
|                | `IDiv` / `IMod`                   | yes | no      |
|                | `IEq` / `ILt` / `IGt`             | yes | no      |
|                | `BEq` / `SEq`                     | yes | no      |
|                | `DAdd` / `DSub` / `DMul` / `DDiv` | yes | no      |
|                | `DLt` / `DGt`                     | yes | no      |
| `BinImm`       | `IAdd` / `ISub` / `IMul` / `IAnd` | yes | no      |
|                | `IDiv` / `IMod`                   | yes | no      |
|                | `IEq` / `ILt` / `IGt`             | yes | no      |
| `LoadConst`    | `Undefined`                       | yes | no      |
|                | `False` / `True`                  | yes | no      |
|                | `Int`                             | yes | no      |
|                | `Double`                          | yes | no      |
|                | `Str`                             | yes | no      |
| `Cast`         | `Int` <-> `Double`                | yes | no      |
|                | `Int` <-> `Bool`                  | yes | no      |
|                | other casts                       | no  | no      |
| `Call`         | native function                   | yes | no      |
|                | bytecode function                 | no  | no      |
| `Sys`          | builtin with a native entry       | yes | no      |
| `Alloc`        |                                   | yes | no      |
| `Store`        |                                   | yes | no      |
| `Load`         |                                   | yes | no      |
| `AddrOf`       |                                   | yes | no      |
| `Lookup`       |                                   | yes | no      |
| `Assert`       |                                   | yes | no      |
| `Noop`         |                                   | yes | no      |

## Future work:

Use something like plan9 asm with slots to abstract away from machines and
build a stencil like JIT, for instance in
`purple-garden-jit/defs/bin.iadd.pgasm`:

```armasm
add %0, %0, %1
```

Gets codegened for all architectures into:

```x86
add    %rax,%rcx
mov    %rcx,%rdi
```

And

```armasm
add x2, x1, x0
```

This is the goal since the current x86 jit encoder implementation is a collection of
spaghetti
