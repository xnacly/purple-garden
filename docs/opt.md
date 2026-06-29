# Optimization Passes

## IR Passes

The IR optimizer is in `purple-garden-opt/src/lib.rs`. Individual passes in
`purple-garden-opt/src/ir/`, all are re-exported from
`purple-garden-opt/src/ir/mod.rs`.

| Pass                  | Scope                | Purpose                                                                  |
| --------------------- | -------------------- | ------------------------------------------------------------------------ |
| `const_fold`          | Function             | Fold constant operations and remove dead constants found during folding. |
| `const_fold_syscalls` | -//-                 | Evaluate pure syscalls with constant inputs.                             |
| `imm_fold`            | -//-                 | Fold single-use integer constants into immediate-form instructions.      |
| `branch_cmp`          | -//-                 | Fold comparison feeding a branch into `BranchCmpImm`.                    |
| `indirect_jump`       | -//-                 | Collapse trivial branch/jump indirections.                               |
| `ret_inline`          | -//-                 | Inline trivial jump-to-return join blocks.                               |
| `tailcall`            | -//-                 | Rewrite tail-call patterns.                                              |
| `addrof_fold`         | Block-local          | Fold single-use `AddrOf` chains into memory consumers.                   |
| `load_store_fold`     | Block-local          | Reserved for load/store forwarding and dead-store style rewrites.        |
| `dce`                 | Function fixed point | Remove dead SSA producers without observable effects.                    |

## Bytecode Peephole Passes

Bytecode peephole passes are orchestrated by `purple-garden-opt/src/lib.rs` and
live in `purple-garden-opt/src/bc/` they are supposed to be the fallback for the IR passes.

| Pass        | Scope    | Purpose                                 |
| ----------- | -------- | --------------------------------------- |
| `self_move` | Peephole | Remove moves from a register to itself. |
| `mov_merge` | Peephole | Merge chained moves.                    |
| `jmp_next`  | Peephole | Remove jumps to the next instruction.   |
