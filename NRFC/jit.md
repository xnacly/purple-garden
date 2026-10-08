# Tracking JIT progress

For current full scope of compilable purple garden script contents, see:
[examples/jitprogress.garden](../examples/jitprogress.garden):

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

This is the goal since the current x86 jit implementation is a collection of
spaghetti

### Calling builtins and native functions without the register file

- today native code calls builtins and native functions through `vm.r` and
  `jit_sys`: memory round trips only for the convention
- `pg_fn` generates two entries per builtin: the interpreter `BuiltinFn` that
  unpacks `vm.r`, and a native entry taking Values in registers, returning the
  value and a trap flag
- the JIT calls the native entry directly, JIT compiled functions get the same
  internal entry, so builtins, native and self calls share one call site
- open: `unsafe` and `with_slots` builtins, more than 5 args, a depth guard
  for native recursion, GC roots once `Gc::collect` traces

## Progress

| IR node        | variant             | x86 | aarch64 |
| -------------- | ------------------- | --- | ------- |
| `Return`       |                     | yes | no      |
| `Jump`         |                     | yes | no      |
| `Branch`       |                     | yes | no      |
| `BranchCmpImm` | `IEq`               | yes | no      |
|                | other ops           | no  | no      |
| `Tail`         | self-recursive      | yes | no      |
|                | other function      | no  | no      |
| `Bin`          | `IAdd`              | yes | no      |
|                | `ISub`              | yes | no      |
|                | `IMul`              | yes | no      |
|                | `IEq`               | yes | no      |
|                | `IDiv`              | yes | no      |
|                | `IMod`              | yes | no      |
|                | `ILt`               | no  | no      |
|                | `IGt`               | no  | no      |
|                | `D*` / `BEq`        | no  | no      |
| `BinImm`       | `IAdd`              | yes | no      |
|                | `ISub`              | yes | no      |
|                | `IEq`               | yes | no      |
|                | `IDiv`              | yes | no      |
|                | `IMod`              | yes | no      |
|                | `IMul`              | no  | no      |
|                | `ILt` / `IGt`       | no  | no      |
|                | `D*` / `BEq`        | no  | no      |
| `LoadConst`    | `Undefined`         | no  | no      |
|                | `False`             | yes | no      |
|                | `True`              | yes | no      |
|                | `Int` (i32-fitting) | yes | no      |
|                | `Int` (full i64)    | no  | no      |
|                | `Double(u64)`       | no  | no      |
|                | `Str(&'c str)`      | no  | no      |
| `Store`        |                     | yes | no      |
| `Load`         |                     | yes | no      |
| `AddrOf`       |                     | yes | no      |
| `Noop`         |                     | yes | no      |
| `Alloc`        |                     | no  | no      |
| `Call`         |                     | no  | no      |
| `Sys`          |                     | no  | no      |
| `Cast`         |                     | no  | no      |
