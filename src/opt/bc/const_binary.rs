use crate::vm::op::Op;

/// const_binary fuses
///
///     LoadI{ dst: a, value: x },
///     LoadI{ dst: b, value: y },
///     bin { dst, lhs: a, rhs: b }
///
/// into
///
///     LoadI{ dst: a, value: x },
///     LoadI{ dst: b, value: y },
///     LoadI { dst, value: x bin y }
///
/// where bin := Add | Sub | Mul | Div
pub fn const_binary(window: &mut [Op]) {
    let [
        Op::LoadI { dst: a, value: x },
        Op::LoadI { dst: b, value: y },
        op,
    ] = window
    else {
        return;
    };

    let (dst, result) = match *op {
        Op::IAdd { dst, lhs, rhs } if lhs == *a && rhs == *b => (dst, x.wrapping_add(*y)),
        Op::ISub { dst, lhs, rhs } if lhs == *a && rhs == *b => (dst, x.wrapping_sub(*y)),
        Op::IMul { dst, lhs, rhs } if lhs == *a && rhs == *b => (dst, x.wrapping_mul(*y)),
        Op::IDiv { dst, lhs, rhs } if lhs == *a && rhs == *b && *y != 0 => {
            (dst, x.wrapping_div(*y))
        }
        _ => return,
    };

    window[2] = Op::LoadI { dst, value: result };

    opt_trace!("const_binary", "fused a constant binary op");
}
