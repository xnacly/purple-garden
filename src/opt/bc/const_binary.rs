use crate::op::Op;

/// const_binary fuses LoadImm{ dst: a, value: x }, LoadImm{ dst: b, value: y }, <BIN> { dst, lhs:
/// a, rhs: b } into LoadImm { dst, value: x <BIN> y }; where BIN := Add | Sub | Mul | Div
pub fn const_binary(window: &mut [Op]) {
    dbg!(&window);
    let [
        Op::LoadImm { dst: a, value: x },
        Op::LoadImm { dst: b, value: y },
        op,
    ] = window
    else {
        return;
    };

    let (dst, result) = match *op {
        Op::Add { dst, lhs, rhs } if lhs == *a && rhs == *b => (dst, x.wrapping_add(*y)),
        Op::Sub { dst, lhs, rhs } if lhs == *a && rhs == *b => (dst, x.wrapping_sub(*y)),
        Op::Mul { dst, lhs, rhs } if lhs == *a && rhs == *b => (dst, x.wrapping_mul(*y)),
        Op::Div { dst, lhs, rhs } if lhs == *a && rhs == *b && *y != 0 => (dst, x.wrapping_div(*y)),
        _ => return,
    };

    window[0] = Op::LoadImm { dst, value: result };
    window[1] = Op::Nop;
    window[2] = Op::Nop;

    opt_trace!("const_binary", "fused a constant binary op");
}
