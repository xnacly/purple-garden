#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
/// Packed version of [Op], since [Op] is 16 bytes and branching on it in the interpreter costs big
/// fetches, making it 4 bytes large results in better locality, more instructions per prefetch and
/// less decoding work, since the vm can simply call [Packed::tag()] to branch on it, for encoding
/// see [encode_packed!] and [super::Op::encode]. Every class has a method to facilitate decoding
pub struct Packed(u32);

impl Packed {
    /// [0..5]
    #[inline(always)]
    pub fn tag(&self) -> super::Tag {
        let tag_raw = (self.0 & 0b0011_1111) as u8;
        unsafe { std::mem::transmute::<u8, super::Tag>(tag_raw) }
    }

    #[inline(always)]
    pub fn to_le_bytes(self) -> [u8; 4] {
        self.0.to_le_bytes()
    }

    /// `width` bits starting at bit `at`
    #[inline(always)]
    fn bits(self, at: u32, width: u32) -> u32 {
        (self.0 >> at) & ((1 << width) - 1)
    }

    #[inline(always)]
    pub fn r(self) -> u8 {
        self.bits(6, 5) as u8
    }

    #[inline(always)]
    pub fn rr(self) -> (u8, u8) {
        (self.r(), self.bits(11, 5) as u8)
    }

    #[inline(always)]
    pub fn rrr(self) -> (u8, u8, u8) {
        let (r1, r2) = self.rr();
        (r1, r2, self.bits(16, 5) as u8)
    }

    #[inline(always)]
    pub fn rri16(self) -> (u8, u8, i16) {
        let (r1, r2) = self.rr();
        (r1, r2, (self.0 as i32 >> 16) as i16)
    }

    #[inline(always)]
    pub fn ri21(self) -> (u8, i32) {
        (self.r(), self.0 as i32 >> 11)
    }

    #[inline(always)]
    pub fn ru21(self) -> (u8, u32) {
        (self.r(), self.0 >> 11)
    }

    #[inline(always)]
    pub fn t16(self) -> u16 {
        self.bits(6, 16) as u16
    }

    #[inline(always)]
    pub fn u26(self) -> u32 {
        self.0 >> 6
    }

    #[inline(always)]
    pub fn rt16(self) -> (u8, u16) {
        (self.r(), self.bits(11, 16) as u16)
    }

    #[inline(always)]
    pub fn rrt16(self) -> (u8, u8, u16) {
        let (r1, r2) = self.rr();
        (r1, r2, (self.0 >> 16) as u16)
    }

    #[inline(always)]
    pub fn ri5t16(self) -> (u8, i8, u16) {
        // bit 15 moved to bit 31, so the arithmetic shift back down sign extends imm5
        let imm = ((self.0 << 16) as i32 >> 27) as i8;
        (self.r(), imm, (self.0 >> 16) as u16)
    }

    #[inline(always)]
    pub fn rru16(self) -> (u8, u8, u16) {
        self.rrt16()
    }

    #[inline(always)]
    pub fn rk1u20(self) -> (u8, super::SwitchKind, u32) {
        // SAFETY: SwitchKind is repr(u8) with a variant for both 1 bit values
        let kind = unsafe { std::mem::transmute::<u8, super::SwitchKind>(self.bits(11, 1) as u8) };
        (self.r(), kind, self.0 >> 12)
    }

    #[inline(always)]
    pub fn rrk1u15(self) -> (u8, u8, super::SwitchKind, u16) {
        let (r1, r2) = self.rr();
        // SAFETY: SwitchKind is repr(u8) with a variant for both 1 bit values
        let kind = unsafe { std::mem::transmute::<u8, super::SwitchKind>(self.bits(16, 1) as u8) };
        (r1, r2, kind, (self.0 >> 17) as u16)
    }

    #[inline(always)]
    pub fn rk2u3u16(self) -> (u8, crate::AllocType, u8, u16) {
        // SAFETY: AllocType is repr(u8) with a variant for every 2 bit value
        let kind = unsafe { std::mem::transmute::<u8, crate::AllocType>(self.bits(11, 2) as u8) };
        (self.r(), kind, 1 << self.bits(13, 3), (self.0 >> 16) as u16)
    }

    #[inline(always)]
    pub fn u5u16(self) -> (u8, u16) {
        (self.r(), (self.0 >> 16) as u16)
    }

    #[inline(always)]
    pub fn u16k1(self) -> (u16, bool) {
        (self.bits(6, 16) as u16, self.bits(22, 1) == 1)
    }
}

/// encode different ops into the new packed format, operator classes are the prefix of the macro
/// invocation
///
/// Every opcode starts with tag6 at 0..5
///
/// r        := r5
/// rr       := r5 r5
/// rrr      := r5 r5 r5
/// rri16    := dst5 lhs5 imm16
/// ri21     := dst5 imm21
/// ru21     := r5 idx21
/// t16      := target16
/// u26      := func26
/// rt16     := cond5 target16
/// rrt16    := lhs5 rhs5 target16
/// ri5t16   := lhs5 imm5 target16
/// rru16    := r5 r5 offset16
/// rk1u20   := subject5 kind1 table20
/// rrk1u15  := dst5 subject5 kind1 table15
/// rk2u3u16 := dst5 kind2 align_log(2)3 size16
/// u5u16    := argc5 idx16
/// u16k1    := idx16 returns_double1
///
/// Use depends on class, first arg is class:
///
/// ```text
/// encode_packed!(r, 12)
/// encode_packed!(rri16, 0, 1, 161)
/// ```
macro_rules! encode_packed {
    (r,$r1:expr) => {
        ($r1 as u32) << 6
    };
    (rr,$r1:expr,$r2:expr) => {
        ($r1 as u32) << 6 | ($r2 as u32) << 11
    };
    (rrr,$r1:expr,$r2:expr,$r3:expr) => {
        ($r1 as u32) << 6 | ($r2 as u32) << 11 | ($r3 as u32) << 16
    };
    (rri16,$r1:expr,$r2:expr,$imm:expr) => {
        ($r1 as u32) << 6 | ($r2 as u32) << 11 | ($imm as u32) << 16
    };
    (ri21,$r1:expr,$imm:expr) => {
        encode_packed!(rr, $r1, $imm)
    };
    (ru21,$r1:expr,$imm:expr) => {
        encode_packed!(ri21, $r1, $imm)
    };
    (t16,$target:expr) => {
        encode_packed!(r, $target)
    };
    (u26,$target:expr) => {
        encode_packed!(r, $target)
    };
    (rt16,$r:expr,$target:expr) => {
        encode_packed!(rr, $r, $target)
    };
    (rrt16,$r1:expr,$r2:expr,$target:expr) => {
        encode_packed!(rrr, $r1, $r2, $target)
    };
    (ri5t16,$r1:expr,$imm:expr,$target:expr) => {
        // imm5 is signed, unmasked the sign bits could and would overwrite target
        encode_packed!(rrr, $r1, ($imm as u32) & 0x1F, $target)
    };
    (rru16,$r1:expr,$r2:expr,$offset:expr) => {
        encode_packed!(rrr, $r1, $r2, $offset)
    };
    (rk1u20,$r1:expr,$kind:expr,$table:expr) => {
        ($r1 as u32) << 6 | ($kind as u32) << 11 | ($table as u32) << 12
    };
    (rrk1u15,$r1:expr,$r2:expr,$kind:expr,$table:expr) => {
        encode_packed!(rrr, $r1, $r2, $kind) | ($table as u32) << 17
    };
    (rk2u3u16,$r1:expr,$kind:expr,$align_log2:expr,$size:expr) => {
        ($r1 as u32) << 6 | ($kind as u32) << 11 | ($align_log2 as u32) << 13 | ($size as u32) << 16
    };
    (u5u16,$argc:expr,$idx:expr) => {
        encode_packed!(rrr, $argc, 0, $idx)
    };
    (u16k1,$idx:expr,$flag:expr) => {
        ($idx as u32) << 6 | ($flag as u32) << 22
    };
}

impl super::Op {
    pub fn encode(self) -> Packed {
        Packed(
            self.tag() as u32
                | match self {
                    // none
                    crate::op::Op::Ret | crate::op::Op::Halt | crate::op::Op::Nop => 0,

                    // r
                    crate::op::Op::Push { src: a }
                    | crate::op::Op::Pop { dst: a }
                    | crate::op::Op::Assert { cond: a } => encode_packed!(r, a),

                    // rr
                    crate::op::Op::CastToInt { dst: a, src: b }
                    | crate::op::Op::CastToDouble { dst: a, src: b }
                    | crate::op::Op::CastToBool { dst: a, src: b }
                    | crate::op::Op::Mov { dst: a, src: b }
                    | crate::op::Op::Push2 { a, b }
                    | crate::op::Op::Pop2 { a, b } => {
                        encode_packed!(rr, a, b)
                    }

                    // rrr
                    crate::op::Op::Push3 { a, b, c } | crate::op::Op::Pop3 { a, b, c } => {
                        encode_packed!(rrr, a, b, c)
                    }
                    crate::op::Op::IAdd { dst, lhs, rhs }
                    | crate::op::Op::ISub { dst, lhs, rhs }
                    | crate::op::Op::IMul { dst, lhs, rhs }
                    | crate::op::Op::IDiv { dst, lhs, rhs }
                    | crate::op::Op::IMod { dst, lhs, rhs }
                    | crate::op::Op::IAnd { dst, lhs, rhs }
                    | crate::op::Op::ILt { dst, lhs, rhs }
                    | crate::op::Op::IGt { dst, lhs, rhs }
                    | crate::op::Op::IEq { dst, lhs, rhs }
                    | crate::op::Op::DAdd { dst, lhs, rhs }
                    | crate::op::Op::DSub { dst, lhs, rhs }
                    | crate::op::Op::DMul { dst, lhs, rhs }
                    | crate::op::Op::DDiv { dst, lhs, rhs }
                    | crate::op::Op::DLt { dst, lhs, rhs }
                    | crate::op::Op::DGt { dst, lhs, rhs }
                    | crate::op::Op::BEq { dst, lhs, rhs }
                    | crate::op::Op::SEq { dst, lhs, rhs } => encode_packed!(rrr, dst, lhs, rhs),

                    // rri16
                    crate::op::Op::IAddI { dst, lhs, imm }
                    | crate::op::Op::ISubI { dst, lhs, imm }
                    | crate::op::Op::IMulI { dst, lhs, imm }
                    | crate::op::Op::IDivI { dst, lhs, imm }
                    | crate::op::Op::IModI { dst, lhs, imm }
                    | crate::op::Op::IAndI { dst, lhs, imm }
                    | crate::op::Op::IEqI { dst, lhs, imm }
                    | crate::op::Op::IGtI { dst, lhs, imm }
                    | crate::op::Op::ILtI { dst, lhs, imm } => {
                        encode_packed!(rri16, dst, lhs, imm)
                    }

                    // ri21
                    crate::op::Op::LoadI { dst, value } => {
                        debug_assert!((-(1 << 20)..1 << 20).contains(&value));
                        encode_packed!(ri21, dst, value)
                    }

                    // ru21
                    crate::op::Op::LoadG { dst, idx } => {
                        debug_assert!(idx < 1 << 21);
                        encode_packed!(ru21, dst, idx)
                    }
                    crate::op::Op::LoadSlot { dst, slot } => encode_packed!(ru21, dst, slot),
                    crate::op::Op::StoreSlot { slot, src } => encode_packed!(ru21, src, slot),

                    // t16
                    crate::op::Op::Jmp { target } => encode_packed!(t16, target),

                    // u26
                    crate::op::Op::Tail { func } | crate::op::Op::Call { func } => {
                        debug_assert!(func < 1 << 26);
                        encode_packed!(u26, func)
                    }

                    // rt16
                    crate::op::Op::JmpT { cond, target } | crate::op::Op::JmpF { cond, target } => {
                        encode_packed!(rt16, cond, target)
                    }

                    // rrt16
                    crate::op::Op::JmpSEq { lhs, rhs, target }
                    | crate::op::Op::JmpSNe { lhs, rhs, target } => {
                        encode_packed!(rrt16, lhs, rhs, target)
                    }

                    // ri5t16
                    crate::op::Op::JmpEqI { lhs, imm, target }
                    | crate::op::Op::JmpNeI { lhs, imm, target } => {
                        debug_assert!((-16..16).contains(&imm));
                        encode_packed!(ri5t16, lhs, imm, target)
                    }

                    // rru16, the value register first, then the base
                    crate::op::Op::Store {
                        base,
                        offset,
                        src: r,
                    }
                    | crate::op::Op::Load {
                        dst: r,
                        base,
                        offset,
                    }
                    | crate::op::Op::AddrOf {
                        dst: r,
                        base,
                        offset,
                    } => {
                        encode_packed!(rru16, r, base, offset)
                    }

                    // rk1u20
                    crate::op::Op::Switch {
                        kind,
                        subject,
                        table,
                    } => encode_packed!(rk1u20, subject, kind, table),

                    // rrk1u15
                    crate::op::Op::Lookup {
                        kind,
                        dst,
                        subject,
                        table,
                    } => {
                        debug_assert!(table < 1 << 15);
                        encode_packed!(rrk1u15, dst, subject, kind, table)
                    }

                    // rk2u3u16
                    crate::op::Op::Alloc {
                        dst,
                        kind,
                        size,
                        align,
                    } => {
                        debug_assert!(align.is_power_of_two() && align <= 128);
                        encode_packed!(rk2u3u16, dst, kind, align.trailing_zeros(), size)
                    }

                    // u5u16
                    crate::op::Op::Sys { idx, argc } => {
                        debug_assert!(argc < 32);
                        encode_packed!(u5u16, argc, idx)
                    }

                    // u16k1
                    crate::op::Op::Native {
                        idx,
                        returns_double,
                    } => encode_packed!(u16k1, idx, returns_double),
                },
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        AllocType,
        op::{Op, SwitchKind, Tag},
    };

    #[test]
    fn decodes_every_class() {
        let enc = Op::encode;
        assert_eq!(enc(Op::Push { src: 31 }).r(), 31);
        assert_eq!(enc(Op::Mov { dst: 1, src: 2 }).rr(), (1, 2));
        assert_eq!(
            enc(Op::IAdd {
                dst: 1,
                lhs: 2,
                rhs: 31
            })
            .rrr(),
            (1, 2, 31)
        );
        assert_eq!(
            enc(Op::IAddI {
                dst: 1,
                lhs: 2,
                imm: -3
            })
            .rri16(),
            (1, 2, -3)
        );
        assert_eq!(
            enc(Op::LoadI {
                dst: 1,
                value: -(1 << 20)
            })
            .ri21(),
            (1, -(1 << 20))
        );
        assert_eq!(
            enc(Op::LoadG {
                dst: 1,
                idx: (1 << 21) - 1
            })
            .ru21(),
            (1, (1 << 21) - 1)
        );
        assert_eq!(enc(Op::Jmp { target: u16::MAX }).t16(), u16::MAX);
        assert_eq!(
            enc(Op::Call {
                func: (1 << 26) - 1
            })
            .u26(),
            (1 << 26) - 1
        );
        assert_eq!(
            enc(Op::JmpF {
                cond: 4,
                target: u16::MAX
            })
            .rt16(),
            (4, u16::MAX)
        );
        assert_eq!(
            enc(Op::JmpSEq {
                lhs: 1,
                rhs: 2,
                target: u16::MAX
            })
            .rrt16(),
            (1, 2, u16::MAX)
        );
        assert_eq!(
            enc(Op::JmpNeI {
                lhs: 1,
                imm: -16,
                target: 3
            })
            .ri5t16(),
            (1, -16, 3)
        );
        assert_eq!(
            enc(Op::JmpEqI {
                lhs: 1,
                imm: 15,
                target: 3
            })
            .ri5t16(),
            (1, 15, 3)
        );
        assert_eq!(
            enc(Op::Load {
                dst: 1,
                base: 2,
                offset: u16::MAX
            })
            .rru16(),
            (1, 2, u16::MAX)
        );
        assert_eq!(
            enc(Op::Switch {
                kind: SwitchKind::Int,
                subject: 3,
                table: u16::MAX
            })
            .rk1u20(),
            (3, SwitchKind::Int, u16::MAX.into())
        );
        assert_eq!(
            enc(Op::Lookup {
                kind: SwitchKind::Str,
                dst: 1,
                subject: 2,
                table: (1 << 15) - 1
            })
            .rrk1u15(),
            (1, 2, SwitchKind::Str, (1 << 15) - 1)
        );
        assert_eq!(
            enc(Op::Alloc {
                dst: 3,
                kind: AllocType::String,
                size: u16::MAX,
                align: 128
            })
            .rk2u3u16(),
            (3, AllocType::String, 128, u16::MAX)
        );
        assert_eq!(
            enc(Op::Sys {
                idx: u16::MAX,
                argc: 31
            })
            .u5u16(),
            (31, u16::MAX)
        );
        assert_eq!(
            enc(Op::Native {
                idx: u16::MAX,
                returns_double: true
            })
            .u16k1(),
            (u16::MAX, true)
        );
    }

    #[test]
    fn encodes_every_class() {
        #[rustfmt::skip]
        let cases = [
            (Op::Ret, Tag::Ret, 0),
            (Op::Push { src: 31 }, Tag::Push, 0x0000_07C0),
            (Op::Mov { dst: 1, src: 2 }, Tag::Mov, 0x0000_1040),
            (Op::IAdd { dst: 1, lhs: 2, rhs: 3 }, Tag::IAdd, 0x0003_1040),
            (Op::IAddI { dst: 1, lhs: 2, imm: -3 }, Tag::IAddI, 0xFFFD_1040),
            (Op::LoadI { dst: 1, value: -3 }, Tag::LoadI, 0xFFFF_E840),
            (Op::LoadG { dst: 1, idx: (1 << 21) - 1 }, Tag::LoadG, 0xFFFF_F840),
            (Op::StoreSlot { slot: 5, src: 30 }, Tag::StoreSlot, 0x0000_2F80),
            (Op::Jmp { target: u16::MAX }, Tag::Jmp, 0x003F_FFC0),
            (Op::Call { func: (1 << 26) - 1 }, Tag::Call, 0xFFFF_FFC0),
            (Op::JmpF { cond: 4, target: u16::MAX }, Tag::JmpF, 0x07FF_F900),
            (Op::JmpSEq { lhs: 1, rhs: 2, target: u16::MAX }, Tag::JmpSEq, 0xFFFF_1040),
            (Op::JmpNeI { lhs: 1, imm: -1, target: 3 }, Tag::JmpNeI, 0x0003_F840),
            (Op::Store { base: 2, offset: u16::MAX, src: 1 }, Tag::Store, 0xFFFF_1040),
            (Op::Switch { kind: SwitchKind::Int, subject: 3, table: u16::MAX }, Tag::Switch, 0x0FFF_F8C0),
            (Op::Lookup { kind: SwitchKind::Int, dst: 1, subject: 2, table: (1 << 15) - 1 }, Tag::Lookup, 0xFFFF_1040),
            (Op::Alloc { dst: 3, kind: AllocType::String, size: u16::MAX, align: 128 }, Tag::Alloc, 0xFFFF_F8C0),
            (Op::Sys { idx: u16::MAX, argc: 31 }, Tag::Sys, 0xFFFF_07C0),
            (Op::Native { idx: u16::MAX, returns_double: true }, Tag::Native, 0x007F_FFC0),
        ];
        for (op, tag, fields) in cases {
            assert_eq!(op.encode().0, tag as u32 | fields, "{op:?}");
        }
    }
}
