# Tracking JIT progress

For current full scope of compilable purple garden script contents, see:
[examples/jitprogress.garden](../examples/jitprogress.garden):

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
|                | `IDiv`              | no  | no      |
|                | `IMod`              | no  | no      |
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
