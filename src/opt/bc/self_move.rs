use crate::op::Op;

/// self_move removes patterns conforming to
///
///     Mov { dst: x, src: x },
///
/// where both dst == src
pub fn self_move(window: &mut [crate::op::Op]) {
    for op in window.iter_mut() {
        if let Op::Mov { dst, src } = op {
            if dst == src {
                *op = Op::Nop;
                opt_trace!("self_move", "removed self_moving Mov");
            }
        }
    }
}
