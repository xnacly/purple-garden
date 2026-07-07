# JIT

This document describes the current Purple Garden Just In Time compiler.

The JIT is a baseline native backend. It is not an optimizing compiler. Its
main job is to remove bytecode dispatch for functions that fit a small
supported IR subset. Compared to the bytecode backend it does not have peephole
optimisations, but all backend do share the optimisations made on the immediate
representation.

If a function uses features not in the supported subset (depends on the
architectures backend), the runtime falls back to bytecode for that function.

Modules:

- `purple-garden-jit/src/lib.rs`: public JIT wrapper and tests.
- `purple-garden-jit/src/x86/mod.rs`: x86-64 admission, register allocation,
  lowering, encoding, and patching.
- `purple-garden-jit/src/aarch64/mod.rs`: same for aarch64
- `purple-garden-jit/src/regalloc.rs`: target-independent linear-scan register
  allocator.
- `purple-garden-runtime/src/jit_helpers.rs`: runtime helper calls used from
  native code, for instance for allocation, bounds checking, vm traps, etc
- `purple-garden-bc/src/lib.rs`: dispatches all jit related code

## Dispatch Model

The bytecode compiler asks the JIT to compile each IR function unless JIT is
disabled (`--no-jit`). A successful native compile records the function as
`CcFunc::Native`. Calls to native functions are emitted as `Op::Sys { idx }`,
where `idx` points at the native page entry in the VM syscall table.

The VM already knows how to call syscalls. Native JIT pages use the same calling
shape as builtins: they receive a pointer to the VM.

If native compilation returns `None`, the bytecode compiler lowers the function
normally. The reaons for this decision can be inspected in the logs enabled
when compiling with `--features trace`.

### Native ABI

On x86-64, the generated function receives `*mut Vm` in `rdi`.

`Vm` is `repr(C)` and its first field is the register file, so the JIT treats
`rdi` as the base address of `vm.r`:

```text
rdi + 0   -> vm.r[0]
rdi + 8   -> vm.r[1]
rdi + 16  -> vm.r[2]
```

Function arguments arrive in `vm.r[0..n]`. At native entry the JIT loads those
slots into physical registers chosen by its own allocator. On return, the JIT
stores the return value back into `vm.r[0]` and emits `ret`.

The syscall convention is : from the VM's point of view, a native function
writes its result to `r0`.

## Compile Pipeline

The current x86 pipeline is:

1. Compute IR liveness in `Jit::compile_func`.
2. Run the x86 support/planning pass.
3. Run JIT register allocation.
4. Emit native code block by block.
5. Patch deferred branch displacements.

The support pass is not only a cheap validation step. It also collects target
constraints that must be known before register allocation. For example, the
current integer division lowering clobbers `rax`, `rcx`, and `rdx`; values live
across that IR position must not be assigned to those registers.

This is why unsupported checks are not purely done during final emission.
Emission happens after register allocation, but some instructions affect the
register allocation decision itself.

## Register Allocation

The JIT uses the shared IR liveness intervals, then maps SSA ids to physical
x86 registers.

The allocator has two register classes:

- caller-saved registers for ordinary values,
- callee-saved registers for values that must survive calls.

It also accepts fixed clobber positions. These are target-specific instruction
positions where a subset of registers is overwritten by the lowering. The x86
`idiv` lowering currently uses this for `rax`, `rcx`, and `rdx`.

Values that cannot be assigned a register become `Location::Stack`. The current
JIT lowering does not handle stack locations, so such functions fall back to
bytecode.

## x86 Instruction Encoding

`x86/mod.rs` has a small hand-written encoder. The `Insn` enum is not a full
assembly IR; it only contains forms the current backend emits.

Most register-register operations use ModRM with `mod = 11`.

VM slot loads/stores use compact memory operands relative to `rdi`:

```asm
mov dst, [rdi + slot * 8]
mov [rdi + slot * 8], src
```

Record field access needs a more general memory form:

```asm
mov dst, [base + offset]
mov [base + offset], src
lea dst, [base + offset]
```

Some x86 base registers require special memory encoding. In particular, `rsp`
and `r12` require a SIB byte even for simple `[base + offset]` addressing. A SIB
byte encodes scale/index/base. For record access we do not need an index; the
SIB form is only needed to satisfy x86 encoding rules for those base registers.


## Runtime Helper Calls

The JIT can call runtime helpers through the platform C ABI. Current helpers
include:

- `jit_trap_div_zero`, used by division-by-zero lowering,
- `jit_alloc`, intended for native allocation support.

Helper calls clobber caller-saved registers. Any instruction lowered as a helper
call must be represented in the planning pass before register allocation, either
as a call site or as fixed clobbers.

# End-to-end Example

```python
fn access_user(user: Record<name:Str age:Int job:Record<name:Str since:Int>>) Int {
    let job = user.job
    job.since
}
```

Invoking the cli with `objdump -d (cargo run --features trace -- -DDd test.garden|psub)` produces:


```shell
[           0.054us] [input::Input::from_file] mmaped the file
[          60.625us] [main] Tokenisation and Parsing done
[         109.430us] [ir::typecheck::Typechecker::node][access_user]: (user: Record<name: Str age: Int job: Record<name: Str since: Int>>) -> Int
[         152.953us] [main] Lowered AST to IR
[         178.960us] [opt::ir::addrof_fold] folded %v1+8 into %v0+24
[         188.802us] [opt::ir::dce] removed dead definition %v1
[         224.605us] [jit::x86] compiled access_user (13 bytes)
[         249.224us] [bc::Cc::cc][access_user] native
[         259.610us] [jit::x86] compiled entry (1 bytes)
[         273.392us] [bc::Cc::cc][entry] native
[         281.924us] [main] Lowered IR to bytecode

/tmp/.psub.eaxBgx:     file format elf64-x86-64


Disassembly of section .text:
```

Highlighted as x86-asm:


```asm
0000000000000000 <jit_access_user>:
   0:   48 8b 47 00             mov    0x0(%rdi),%rax
   4:   48 8b 40 18             mov    0x18(%rax),%rax
   8:   48 89 47 00             mov    %rax,0x0(%rdi)
   c:   c3                      ret

000000000000000d <jit_entry>:
   d:   c3                      ret
```
