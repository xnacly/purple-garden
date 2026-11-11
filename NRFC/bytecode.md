# Purple garden bytecode

This is a document for designing and iterating on purple gardens bytecode
layout. The goal is to have small but expressive bytecode, which means we have
to cut some corners.

## Previous state

Before the packed rework (#74) bytecode was very fat, op was 16 bytes:

```asm
       : 74    let op = unsafe { *instructions.add(pc) };
  3.18 :   c252c:  movzbl 0x0(%rbp,%r13,8),%edx
 26.12 :   c2532:  movzbl 0x1(%rbp,%r13,8),%r15d
  1.27 :   c2538:  movzwl 0x2(%rbp,%r13,8),%ebx
  1.20 :   c253e:  mov    %ebx,%esi
  5.87 :   c2540:  shr    $0x8,%esi
  0.80 :   c2543:  movslq 0x4(%rbp,%r13,8),%rcx
       : 81    Op::LoadG { dst, idx } => unsafe { r_mut!(dst) = *globals.add(idx as usize) },
  0.93 :   c2548:  mov    %ecx,%eax
       : 83    match op {
```

Meaning:

```asm
movzbl 0x0(...), %edx   ; opcode/tag byte
movzbl 0x1(...), %r15d  ; first operand byte
movzwl 0x2(...), %ebx   ; next packed operands
movslq 0x4(...), %rcx   ; 32-bit payload/immediate
```

## Packed ops

This a lot of work to do for each instruction, especially since not all
instructions require the same payload form, etc, thus the vm now executes a
packed 32bit op (`runtime::op::Packed`) and the previously used
`runtime::op::Op` moved one layer up is a MIR for bc so to say, `Cc::finalize`
encodes it via `Op::encode`.

Instead of having a general decoding in the dispatch loop, the decoding is
instruction specific and done on demand in the handler of the specific variant.

Instead of requiring a byte for the tag, we use 6 bits (2^6=64) to encode the
opcode. Some instructions require three operands, to make this work the amount
of registers was reduced from 64 to 32 and each operand is represented using 5
bits (2^5=32). For example:

```text
rrr:     op6 | rd5 | rn5 | rm5 | spare11
rri16:   op6 | rd5 | rn5 | imm16i
ri21:    op6 | rd5 | imm21i
rru16:   op6 | rd5 | base5 | offset16u
rt16:    op6 | cond5 | target16u | spare5
u26:     op6 | func26u
```

### Encoding all variants

The opcode selects the instruction class used to decode its remaining bits. Bit
ranges below are `[high:low]`, and `op` is `[5:0]`.

Fields pack left-to-right from bit 6 in the order listed; a bare register
operand takes 5 bits, nameN takes N. Postfix i = signed (arithmetic shift), u =
unsigned (logical shift), k = an enum (`SwitchKind`, `AllocType`), t = a jump
target. 

Each class has a `Packed` method returning its fields and a module in
`runtime::vm::handlers` with the handler for decoding and running it

| Class      | Layout                                   | Used by                                    |
| ---------- | ---------------------------------------- | ------------------------------------------ |
| -          |                                          | `Ret`, `Halt`, `Nop`                       |
| `r`        | `r[10:6]`                                | `Push`, `Pop`, `Assert`                    |
| `rr`       | `r[10:6] r[15:11]`                       | `Mov`, `Push2`, `Pop2`, `CastTo*`          |
| `rrr`      | `r[10:6] r[15:11] r[20:16]`              | int, double and eq binops, `Push3`, `Pop3` |
| `rri16`    | `r[10:6] r[15:11] i16[31:16]`            | int binops with imm                        |
| `ri21`     | `r[10:6] i21[31:11]`                     | `LoadI`                                    |
| `ru21`     | `r[10:6] u21[31:11]`                     | `LoadG`, `LoadSlot`, `StoreSlot`           |
| `t16`      | `t16[21:6]`                              | `Jmp`                                      |
| `u26`      | `u26[31:6]`                              | `Call`, `Tail`                             |
| `rt16`     | `r[10:6] t16[26:11]`                     | `JmpT`, `JmpF`                             |
| `rrt16`    | `r[10:6] r[15:11] t16[31:16]`            | `JmpSEq`, `JmpSNe`                         |
| `ri5t16`   | `r[10:6] i5[15:11] t16[31:16]`           | `JmpEqI`, `JmpNeI`                         |
| `rru16`    | `r[10:6] r[15:11] u16[31:16]`            | `Store`, `Load`, `AddrOf`                  |
| `rk1u20`   | `r[10:6] k1[11] u20[31:12]`              | `Switch`                                   |
| `rrk1u15`  | `r[10:6] r[15:11] k1[16] u15[31:17]`     | `Lookup`                                   |
| `rk2u3u16` | `r[10:6] k2[12:11] u3[15:13] u16[31:16]` | `Alloc`, u3 is log2 of the alignment       |
| `u5u16`    | `u5[10:6] u16[31:16]`                    | `Sys`, u5 is argc                          |
| `u16k1`    | `u16[21:6] k1[22]`                       | `Native`, k1 is returns_double             |

### Opcode assignment

|        Opcode | Instruction    | Class      | Operands / notes                                                                                   |
| ------------: | -------------- | ---------- | -------------------------------------------------------------------------------------------------- |
|        `0x00` | `IAdd`         | `rrr`      | `rd, lhs, rhs`                                                                                     |
|        `0x01` | `IAddI`        | `rri16`    | `rd, lhs, imm16i`, previous imm32i, wider immediates use the register form via a temp              |
|        `0x02` | `ISub`         | `rrr`      | `rd, lhs, rhs`                                                                                     |
|        `0x03` | `ISubI`        | `rri16`    | `rd, lhs, imm16i`                                                                                  |
|        `0x04` | `IMul`         | `rrr`      | `rd, lhs, rhs`                                                                                     |
|        `0x05` | `IMulI`        | `rri16`    | `rd, lhs, imm16i`                                                                                  |
|        `0x06` | `IDiv`         | `rrr`      | `rd, lhs, rhs`                                                                                     |
|        `0x07` | `IDivI`        | `rri16`    | `rd, lhs, imm16i`                                                                                  |
|        `0x08` | `IMod`         | `rrr`      | `rd, lhs, rhs`                                                                                     |
|        `0x09` | `IModI`        | `rri16`    | `rd, lhs, imm16i`                                                                                  |
|        `0x0a` | `IAnd`         | `rrr`      | `rd, lhs, rhs`                                                                                     |
|        `0x0b` | `IAndI`        | `rri16`    | `rd, lhs, imm16i`                                                                                  |
|        `0x0c` | `ILt`          | `rrr`      | `rd, lhs, rhs`                                                                                     |
|        `0x0d` | `IGt`          | `rrr`      | `rd, lhs, rhs`                                                                                     |
|        `0x0e` | `IEq`          | `rrr`      | `rd, lhs, rhs`                                                                                     |
|        `0x0f` | `IEqI`         | `rri16`    | `rd, lhs, imm16i`                                                                                  |
|        `0x10` | `IGtI`         | `rri16`    | `rd, lhs, imm16i`                                                                                  |
|        `0x11` | `ILtI`         | `rri16`    | `rd, lhs, imm16i`                                                                                  |
|        `0x12` | `DAdd`         | `rrr`      | `rd, lhs, rhs`                                                                                     |
|        `0x13` | `DSub`         | `rrr`      | `rd, lhs, rhs`                                                                                     |
|        `0x14` | `DMul`         | `rrr`      | `rd, lhs, rhs`                                                                                     |
|        `0x15` | `DDiv`         | `rrr`      | `rd, lhs, rhs`                                                                                     |
|        `0x16` | `DLt`          | `rrr`      | `rd, lhs, rhs`                                                                                     |
|        `0x17` | `DGt`          | `rrr`      | `rd, lhs, rhs`                                                                                     |
|        `0x18` | `BEq`          | `rrr`      | `rd, lhs, rhs`                                                                                     |
|        `0x19` | `SEq`          | `rrr`      | `rd, lhs, rhs`, strings are interned so this compares payload addresses                            |
|        `0x1a` | `Mov`          | `rr`       | `rd, src`                                                                                          |
|        `0x1b` | `LoadI`        | `ri21`     | `rd, imm21i`, previous imm32i, wider ints come from the globals via `LoadG`                        |
|        `0x1c` | `LoadG`        | `ru21`     | `rd, global_index21u`                                                                              |
|        `0x1d` | `Jmp`          | `t16`      | `target16u`, the proposal had target26u, 16 bits like every other jump                             |
|        `0x1e` | `JmpT`         | `rt16`     | `cond, target16u`                                                                                  |
|        `0x1f` | `JmpF`         | `rt16`     | `cond, target16u`                                                                                  |
|        `0x20` | `JmpEqI`       | `ri5t16`   | `lhs, imm5i, target16u`, rhs stayed an immediate, wider ones compare into a temp via `IEqI` + jump |
|        `0x21` | `JmpNeI`       | `ri5t16`   | `lhs, imm5i, target16u`, see above                                                                 |
|        `0x22` | `JmpSEq`       | `rrt16`    | `lhs, rhs, target16u`                                                                              |
|        `0x23` | `JmpSNe`       | `rrt16`    | `lhs, rhs, target16u`                                                                              |
|        `0x24` | `Switch`       | `rk1u20`   | `subject, kind1, table20u`, indexes `Vm::switch_tables`, misses fall through to the next op        |
|        `0x25` | `Lookup`       | `rrk1u15`  | `rd, subject, kind1, table15u`, indexes `Vm::lookup_tables`                                        |
|        `0x26` | `Tail`         | `u26`      | `func26u`                                                                                          |
|        `0x27` | `Call`         | `u26`      | `func26u`                                                                                          |
|        `0x28` | `Sys`          | `u5u16`    | `argc5u, syscall16u`, argc only feeds a debug check that builtins don't clobber past their args    |
|        `0x29` | `Native`       | `u16k1`    | `syscall16u, returns_double1`                                                                      |
|        `0x2a` | `Push`         | `r`        | `src`                                                                                              |
|        `0x2b` | `Push2`        | `rr`       | `a, b`                                                                                             |
|        `0x2c` | `Push3`        | `rrr`      | `a, b, c`                                                                                          |
|        `0x2d` | `Pop`          | `r`        | `dst`                                                                                              |
|        `0x2e` | `Pop2`         | `rr`       | `a, b`                                                                                             |
|        `0x2f` | `Pop3`         | `rrr`      | `a, b, c`                                                                                          |
|        `0x30` | `StoreSlot`    | `ru21`     | `src, slot21u`                                                                                     |
|        `0x31` | `LoadSlot`     | `ru21`     | `rd, slot21u`                                                                                      |
|        `0x32` | `CastToInt`    | `rr`       | `rd, src`                                                                                          |
|        `0x33` | `CastToDouble` | `rr`       | `rd, src`                                                                                          |
|        `0x34` | `CastToBool`   | `rr`       | `rd, src`                                                                                          |
|        `0x35` | `Alloc`        | `rk2u3u16` | `rd, kind2, log2_align3u, size16u`, previous size18u, align is a power of two up to 128            |
|        `0x36` | `Store`        | `rru16`    | `src, base, offset16u`                                                                             |
|        `0x37` | `Load`         | `rru16`    | `rd, base, offset16u`                                                                              |
|        `0x38` | `AddrOf`       | `rru16`    | `rd, base, offset16u`                                                                              |
|        `0x39` | `Ret`          | -          |                                                                                                    |
|        `0x3a` | `Assert`       | `r`        | `cond`                                                                                             |
|        `0x3b` | `Halt`         | -          | `bc::Cc::finalize` appends it, the vm stops on it                                                  |
|        `0x3c` | `Nop`          | -          |                                                                                                    |
| `0x3d`-`0x3f` | -              |            | reserved                                                                                           |

### Decoding

Every handler decodes its own class and ends by tail calling (`become`) the
handler of the next op, so every handler owns its indirect jump and the branch
predictor gets one per op instead of one shared one. `IAdd` (`rrr`) compiles
to:

```asm
mov    %ecx,%eax              ; op
shr    $0x8,%eax
and    $0xf8,%eax             ; lhs * 8: r[15:11] shifted to a Value offset
mov    %ecx,%r8d
shr    $0xd,%r8d
and    $0xf8,%r8d             ; rhs * 8: r[20:16]
mov    (%rdi,%r8,1),%r8       ; r[rhs], registers sit at offset 0 of the vm
add    (%rdi,%rax,1),%r8      ; + r[lhs]
shr    $0x3,%ecx
and    $0xf8,%ecx             ; dst * 8: r[10:6]
mov    %r8,(%rdi,%rcx,1)      ; r[dst] =
mov    0x4(%rsi,%rdx,4),%ecx  ; fetch the next op
inc    %rdx                   ; pc + 1
mov    %ecx,%eax
and    $0x3f,%eax             ; its tag
lea    0x1c1380(%rip),%r8     ; handler table
jmp    *(%r8,%rax,8)          ; tail call its handler
```

Compared to the previous decode there is no full op load and no byte by byte
unpacking anymore, the fields are shifted out of the one u32 already in a
register.
