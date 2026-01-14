use crate::op::Op;

/// const_add fuses LoadImm{ dst: a, value: x }, LoadImm{ dst: b, value: y }, Add { dst, lhs: a, rhs: b } into LoadImm { dst, value: x+y }
pub fn const_add(window: &mut [Op]) {
    if let [
        Op::LoadImm { dst: a, value: x },
        Op::LoadImm { dst: b, value: y },
        Op::Add { dst, lhs, rhs },
    ] = window
    {
        if lhs == a && rhs == b {
            window[0] = Op::LoadImm {
                dst: *dst,
                value: *x + *y,
            };
            window[1] = Op::Nop;
            window[2] = Op::Nop;
            opt_trace!("const_add", "fused two imm loads and an add");
        }
    }
}
