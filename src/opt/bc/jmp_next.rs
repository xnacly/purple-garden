use crate::vm::op::Op;

/// jmp_next removes the pattern of jmps to the next instruction,
/// which can just be a falltrough
///
/// ```text
/// Jmp { target: t, },
/// ```
///
/// where t == pos(Jmp)+1
pub fn jmp_next(pos: usize, window: &mut [Op]) {
    // since opt::bc's window logic will move over all instructions as window[0] eventually, a
    // single check for window[0] is enough
    if let [Op::Jmp { target }, _] = window
        && *target as usize == pos + 1
    {
        window[0] = Op::Nop;
        opt_trace!("jmp_next", "removed next instruction jump");
    }
}

#[cfg(test)]
mod tests {
    use super::jmp_next;
    use crate::vm::op::Op;

    #[test]
    fn jmp_next_removes_adjacent_forward_jump() {
        let mut bc = vec![Op::Jmp { target: 1 }, Op::Ret];
        jmp_next(0, &mut bc);
        assert_eq!(bc, vec![Op::Nop, Op::Ret]);
    }

    #[test]
    fn jmp_next_removes_adjacent_forward_jump_at_nonzero_pos() {
        let mut window = vec![Op::Jmp { target: 6 }, Op::Ret];
        jmp_next(5, &mut window);
        assert_eq!(window, vec![Op::Nop, Op::Ret]);
    }

    #[test]
    fn jmp_next_leaves_far_forward_jump() {
        let mut bc = vec![Op::Jmp { target: 5 }, Op::Ret];
        jmp_next(0, &mut bc);
        assert_eq!(bc, vec![Op::Jmp { target: 5 }, Op::Ret]);
    }

    #[test]
    fn jmp_next_leaves_backward_jump() {
        let mut bc = vec![Op::Jmp { target: 0 }, Op::Ret];
        // pos=5 means the jump goes back 5 target is not pos+1.
        jmp_next(5, &mut bc);
        assert_eq!(bc, vec![Op::Jmp { target: 0 }, Op::Ret]);
    }

    #[test]
    fn jmp_next_leaves_self_jump() {
        // Infinite loop (Jmp to self) is target == pos, not pos+1; must
        // not be Nop'd or we silently break the program's semantics.
        let mut bc = vec![Op::Jmp { target: 3 }, Op::Ret];
        jmp_next(3, &mut bc);
        assert_eq!(bc, vec![Op::Jmp { target: 3 }, Op::Ret]);
    }

    #[test]
    fn jmp_next_leaves_conditional_jmpt() {
        // JmpT{cond, target: pos+1} is also semantically redundant (both
        // taken/fallthrough paths land at pos+1), but jmp_next only
        // matches unconditional Jmp. If extended later, flip this test
        // to assert Nop.
        let mut bc = vec![Op::JmpT { cond: 1, target: 1 }, Op::Ret];
        jmp_next(0, &mut bc);
        assert_eq!(bc, vec![Op::JmpT { cond: 1, target: 1 }, Op::Ret]);
    }

    #[test]
    fn jmp_next_leaves_non_jmp_at_window_head() {
        let mut bc = vec![Op::Mov { dst: 0, src: 1 }, Op::Ret];
        jmp_next(0, &mut bc);
        assert_eq!(bc, vec![Op::Mov { dst: 0, src: 1 }, Op::Ret]);
    }
}
