use purple_garden_runtime::op::Op;

/// `self_move` removes patterns conforming to
///
/// ```text
/// Mov { dst: x, src: x },
/// ```
///
/// where both dst == src
pub fn self_move(window: &mut [Op]) {
    if let [Op::Mov { dst, src }, ..] = window
        && dst == src
    {
        window[0] = Op::Nop;
        purple_garden_shared::trace!("[opt::self_move] removed self_moving Mov");
    }
}

#[cfg(test)]
mod tests {
    use super::self_move;
    use purple_garden_runtime::op::Op;

    #[test]
    fn removes_self_move() {
        let mut bc = vec![Op::Mov { src: 64, dst: 64 }, Op::Ret];
        self_move(&mut bc);
        assert_eq!(bc, vec![Op::Nop, Op::Ret]);
    }

    #[test]
    fn handles_single_instruction_window() {
        let mut bc = vec![Op::Mov { src: 64, dst: 64 }];
        self_move(&mut bc);
        assert_eq!(bc, vec![Op::Nop]);
    }
}
