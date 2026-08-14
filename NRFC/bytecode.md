# Purple garden bytecode

This is a document for designing and iterating on purple gardens bytecode
layout. The goal is to have small but expressive bytecode, which means we have
to cut some corners.

## Current state

Current bytecode is very fat, op is 8 bytes (64bit):

```rust
pub enum Op {
    IAdd {
        dst: u8,
        lhs: u8,
        rhs: u8,
    },
    IAddI {
        dst: u8,
        lhs: u8,
        imm: i32,
    },

    // ...

    Mov {
        dst: u8,
        src: u8,
    },

    LoadI {
        dst: u8,
        value: i32,
    },
    LoadG {
        dst: u8,
        idx: u32,
    },

    Jmp {
        target: u16,
    },
    JmpT {
        cond: u8,
        target: u16,
    },
    JmpF {
        cond: u8,
        target: u16,
    },
    JmpEqI {
        lhs: u8,
        imm: i32,
        target: u16,
    },

    // ...

    Tail {
        func: u32,
    },

    Call {
        func: u32,
    },
    Sys {
        idx: u16,
    },
    // ...
    Push {
        src: u8,
    },
    // ...
    Pop {
        dst: u8,
    },
    // ...
    CastToInt {
        dst: u8,
        src: u8,
    },
    // ...
    Alloc {
        dst: u8,
        kind: AllocType,
        size: u32,
        align: u8,
    },
    Store {
        base: u8,
        offset: u32,
        src: u8,
    },
    Load {
        dst: u8,
        base: u8,
        offset: u32,
    },
    AddrOf {
        dst: u8,
        base: u8,
        offset: u32,
    },
    Ret,
    Nop,
}

#[cfg(test)]
mod op_test {
    #[test]
    fn op_size_8_byte() {
        assert_eq!(std::mem::size_of::<crate::op::Op>(), 8);
    }
}
```

In the virtual machine, the structure decode compiles to:

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

## Proposal?

This a lot of work to do for each instruction, especially since not all
instructions require the same payload form, etc, thus I think the following
packed 32bit representation would improve performance.

Instead of having a general decoding in the dispatch loop, the decoding is
instruction specific and done on demand in the handler of the specific variant.

Instead of requiring a byte for the tag, as the tag is represented right now,
we will use 6 bits (2^6=64) to encode the opcode. Some instructions require
three operands, to make this work the amount of registers will be reduced from
64 to 32 and each operand will be represented using 5 bits (2^5=32). For
example:

```text
RRR:     op6 | rd5 | rn5 | rm5 | spare11
RI16:    op6 | rd5 | rn5 | imm16
LoadI:   op6 | rd5 | imm21
Mem:     op6 | rd5 | base5 | offset16
Branch:  op6 | cond5 | target21
Jump:    op6 | target26
```

### Encoding all variants

The opcode selects the instruction class used to decode its remaining bits;
there is no separate class tag. Bit ranges below are `[high:low]`, and `op`
always means `[5:0]`.

#### Instruction classes

```text
NULL:  [5:0] op | [31:6] reserved
R:     [5:0] op | [10:6] r | [31:11] reserved
RR:    [5:0] op | [10:6] rd | [15:11] rn | [31:16] reserved
RRR:   [5:0] op | [10:6] rd | [15:11] rn | [20:16] rm | [31:21] reserved
RI16:  [5:0] op | [10:6] rd | [15:11] rn | [31:16] signed imm16
RI21:  [5:0] op | [10:6] rd | [31:11] signed imm21
MEM:   [5:0] op | [10:6] value | [15:11] base | [31:16] unsigned offset16
BR:    [5:0] op | [10:6] cond | [31:11] absolute target21
JMP:   [5:0] op | [31:6] absolute target26
SYS:   [5:0] op | [21:6] syscall16 | [31:22] reserved
ALLOC: [5:0] op | [10:6] rd | [13:11] kind | [18:14] align_log2 | [31:19] size13
BRI5:  [5:0] op | [10:6] lhs | [15:11] signed imm5 | [31:16] absolute target16
```

Sys 10bits reserved to enable inlining a0, a1 into the instruction itself (registers).

### Opcode assignment

|        Opcode | Instruction    | Class | Operands / notes                              |
| ------------: | -------------- | ----- | --------------------------------------------- |
|        `0x00` | `Nop`          | NULL  |                                               |
|        `0x01` | `Ret`          | NULL  |                                               |
|        `0x02` | `Mov`          | RR    | `rd, src`                                     |
|        `0x03` | `IAdd`         | RRR   | `rd, lhs, rhs`                                |
|        `0x04` | `ISub`         | RRR   | `rd, lhs, rhs`                                |
|        `0x05` | `IMul`         | RRR   | `rd, lhs, rhs`                                |
|        `0x06` | `IDiv`         | RRR   | `rd, lhs, rhs`                                |
|        `0x07` | `IMod`         | RRR   | `rd, lhs, rhs`                                |
|        `0x08` | `ILt`          | RRR   | `rd, lhs, rhs`                                |
|        `0x09` | `IGt`          | RRR   | `rd, lhs, rhs`                                |
|        `0x0a` | `IEq`          | RRR   | `rd, lhs, rhs`                                |
|        `0x0b` | `DAdd`         | RRR   | `rd, lhs, rhs`                                |
|        `0x0c` | `DSub`         | RRR   | `rd, lhs, rhs`                                |
|        `0x0d` | `DMul`         | RRR   | `rd, lhs, rhs`                                |
|        `0x0e` | `DDiv`         | RRR   | `rd, lhs, rhs`                                |
|        `0x0f` | `DLt`          | RRR   | `rd, lhs, rhs`                                |
|        `0x10` | `DGt`          | RRR   | `rd, lhs, rhs`                                |
|        `0x11` | `BEq`          | RRR   | `rd, lhs, rhs`                                |
|        `0x12` | `IAddI`        | RI16  | `rd, lhs, imm16`                              |
|        `0x13` | `ISubI`        | RI16  | `rd, lhs, imm16`                              |
|        `0x14` | `IMulI`        | RI16  | `rd, lhs, imm16`                              |
|        `0x15` | `IDivI`        | RI16  | `rd, lhs, imm16`                              |
|        `0x16` | `IModI`        | RI16  | `rd, lhs, imm16`                              |
|        `0x17` | `IEqI`         | RI16  | `rd, lhs, imm16`                              |
|        `0x18` | `IGtI`         | RI16  | `rd, lhs, imm16`                              |
|        `0x19` | `ILtI`         | RI16  | `rd, lhs, imm16`                              |
|        `0x1a` | `LoadI`        | RI21  | `rd, imm21`                                   |
|        `0x1b` | `LoadG`        | RI21  | `rd, global_index16`; bits `[31:27]` reserved |
|        `0x1c` | `Jmp`          | JMP   | `target26`                                    |
|        `0x1d` | `JmpT`         | BR    | `cond, target21`                              |
|        `0x1e` | `JmpF`         | BR    | `cond, target21`                              |
|        `0x1f` | `JmpEqI`       | BRI5  | `lhs, imm5, target16`                         |
|        `0x20` | `JmpNeI`       | BRI5  | `lhs, imm5, target16`                         |
|        `0x21` | `Tail`         | JMP   | `func26`                                      |
|        `0x22` | `Call`         | JMP   | `func26`                                      |
|        `0x23` | `Sys`          | SYS   | `syscall16`                                   |
|        `0x24` | `Push`         | R     | `src`                                         |
|        `0x25` | `Push2`        | RR    | `a, b`                                        |
|        `0x26` | `Push3`        | RRR   | `a, b, c`                                     |
|        `0x27` | `Pop`          | R     | `dst`                                         |
|        `0x28` | `Pop2`         | RR    | `a, b`                                        |
|        `0x29` | `Pop3`         | RRR   | `a, b, c`                                     |
|        `0x2a` | `CastToInt`    | RR    | `dst, src`                                    |
|        `0x2b` | `CastToDouble` | RR    | `dst, src`                                    |
|        `0x2c` | `CastToBool`   | RR    | `dst, src`                                    |
|        `0x2d` | `Alloc`        | ALLOC | `rd, kind3, align_log2, size13`               |
|        `0x2e` | `Store`        | MEM   | `src, base, offset16`                         |
|        `0x2f` | `Load`         | MEM   | `dst, base, offset16`                         |
|        `0x30` | `AddrOf`       | MEM   | `dst, base, offset16`                         |
| `0x31`-`0x3f` | -              | -     | reserved                                      |
