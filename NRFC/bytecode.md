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

Fields pack left-to-right from bit 6 in the order listed; a bare register
operand takes 5 bits, nameN takes N. Postfix i = signed (arithmetic shift), u =
unsigned (logical shift).

### Opcode assignment

|        Opcode | Instruction    | Operands / notes                                                                                                                |
| ------------: | -------------- | ------------------------------------------------------------------------------------------------------------------------------- |
|        `0x00` | `Nop`          |                                                                                                                                 |
|        `0x01` | `Ret`          |                                                                                                                                 |
|        `0x02` | `Mov`          | `rd, src`                                                                                                                       |
|        `0x03` | `IAdd`         | `rd, lhs, rhs`                                                                                                                  |
|        `0x04` | `ISub`         | `rd, lhs, rhs`                                                                                                                  |
|        `0x05` | `IMul`         | `rd, lhs, rhs`                                                                                                                  |
|        `0x06` | `IDiv`         | `rd, lhs, rhs`                                                                                                                  |
|        `0x07` | `IMod`         | `rd, lhs, rhs`                                                                                                                  |
|        `0x08` | `ILt`          | `rd, lhs, rhs`                                                                                                                  |
|        `0x09` | `IGt`          | `rd, lhs, rhs`                                                                                                                  |
|        `0x0a` | `IEq`          | `rd, lhs, rhs`                                                                                                                  |
|        `0x0b` | `DAdd`         | `rd, lhs, rhs`                                                                                                                  |
|        `0x0c` | `DSub`         | `rd, lhs, rhs`                                                                                                                  |
|        `0x0d` | `DMul`         | `rd, lhs, rhs`                                                                                                                  |
|        `0x0e` | `DDiv`         | `rd, lhs, rhs`                                                                                                                  |
|        `0x0f` | `DLt`          | `rd, lhs, rhs`                                                                                                                  |
|        `0x10` | `DGt`          | `rd, lhs, rhs`                                                                                                                  |
|        `0x11` | `BEq`          | `rd, lhs, rhs`                                                                                                                  |
|        `0x12` | `IAddI`        | `rd, lhs, imm16i`                                                                                                               |
|        `0x13` | `ISubI`        | `rd, lhs, imm16i`                                                                                                               |
|        `0x14` | `IMulI`        | `rd, lhs, imm16i`                                                                                                               |
|        `0x15` | `IDivI`        | `rd, lhs, imm16i`                                                                                                               |
|        `0x16` | `IModI`        | `rd, lhs, imm16i`                                                                                                               |
|        `0x17` | `IEqI`         | `rd, lhs, imm16i`                                                                                                               |
|        `0x18` | `IGtI`         | `rd, lhs, imm16i`                                                                                                               |
|        `0x19` | `ILtI`         | `rd, lhs, imm16i`                                                                                                               |
|        `0x1a` | `LoadI`        | `rd, imm21i`, previous imm32i, now imm21i                                                                                       |
|        `0x1b` | `LoadG`        | `rd, global_index21u`                                                                                                           |
|        `0x1c` | `Jmp`          | `target26u`                                                                                                                     |
|        `0x1d` | `JmpT`         | `cond, target21u`                                                                                                               |
|        `0x1e` | `JmpF`         | `cond, target21u`                                                                                                               |
|        `0x1f` | `JmpEqI`       | `lhs, rhs, target16u`, both require a change to the vm impl, rhs is now r, not imm, also requires a change to `opt::branch_cmp` |
|        `0x20` | `JmpNeI`       | `lhs, rhs, target16u`, see above and `opt::branch_cmp` pass is skipped when the target exceeds `target16u`                      |
|        `0x21` | `Tail`         | `func26u`                                                                                                                       |
|        `0x22` | `Call`         | `func26u`                                                                                                                       |
|        `0x23` | `Sys`          | `syscall16u` 10bits reserved to enable inlining a0, a1 into the instr                                                           |
|        `0x24` | `Push`         | `src`                                                                                                                           |
|        `0x25` | `Push2`        | `a, b`                                                                                                                          |
|        `0x26` | `Push3`        | `a, b, c`                                                                                                                       |
|        `0x27` | `Pop`          | `dst`                                                                                                                           |
|        `0x28` | `Pop2`         | `a, b`                                                                                                                          |
|        `0x29` | `Pop3`         | `a, b, c`                                                                                                                       |
|        `0x2a` | `CastToInt`    | `dst, src`                                                                                                                      |
|        `0x2b` | `CastToDouble` | `dst, src`                                                                                                                      |
|        `0x2c` | `CastToBool`   | `dst, src`                                                                                                                      |
|        `0x2d` | `Alloc`        | `rd, kind3u, size18u`                                                                                                           |
|        `0x2e` | `Store`        | `src, base, offset16u`                                                                                                          |
|        `0x2f` | `Load`         | `dst, base, offset16u`                                                                                                          |
|        `0x30` | `AddrOf`       | `dst, base, offset16u`                                                                                                          |
| `0x31`-`0x3f` | -              | reserved                                                                                                                        |
