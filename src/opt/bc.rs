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

/// self_move removes patterns conforming to
///
/// ```text
/// Mov { dst: x, src: x },
/// ```
///
/// where both dst == src
pub fn self_move(window: &mut [Op]) {
    // PERF: remove the loop here and replace it with two branches
    for op in window.iter_mut() {
        if let Op::Mov { dst, src } = op
            && dst == src
        {
            *op = Op::Nop;
            opt_trace!("self_move", "removed self_moving Mov");
        }
    }
}

/// mov_merge merges two movs which can be represented as a single mov:
///
/// ```text
/// Mov { dst: 8, src: 0 }
/// Mov { dst: 2, src: 8 }
/// ```
///
/// Into
///
/// ```text
/// Mov { dst: 2, src: 0 }
/// ```
///
/// This is a fallback for the copy propagation missing some movs between blocks
pub fn mov_merge(window: &mut [Op]) {
    let [
        Op::Mov {
            dst: m0dst,
            src: m0src,
        },
        Op::Mov {
            dst: m1dst,
            src: m1src,
        },
    ] = window
    else {
        return;
    };
    if m0dst != m1src {
        return;
    }

    let (dst, src) = (*m1dst, *m0src);

    window[0] = Op::Nop;
    window[1] = Op::Mov { dst, src };
    opt_trace!("mov_merge", "merged two movs");
}

#[cfg(test)]
mod bc_test {
    use crate::vm::op::Op;

    #[test]
    fn self_move() {
        let mut bc = vec![
            Op::Mov { src: 64, dst: 64 },
            Op::Mov { src: 64, dst: 64 },
            Op::Mov { src: 64, dst: 64 },
        ];
        crate::opt::bc::self_move(&mut bc);
        assert_eq!(bc, vec![Op::Nop, Op::Nop, Op::Nop])
    }

    #[test]
    fn mov_merge() {
        let mut bc = vec![Op::Mov { dst: 8, src: 0 }, Op::Mov { dst: 2, src: 8 }];
        crate::opt::bc::mov_merge(&mut bc);
        assert_eq!(bc, vec![Op::Nop, Op::Mov { dst: 2, src: 0 }])
    }

    #[test]
    fn mov_merge_non_mergable() {
        let mut bc = vec![Op::Mov { dst: 7, src: 0 }, Op::Mov { dst: 2, src: 8 }];
        crate::opt::bc::mov_merge(&mut bc);
        assert_eq!(
            bc,
            vec![Op::Mov { dst: 7, src: 0 }, Op::Mov { dst: 2, src: 8 }]
        )
    }

    #[test]
    fn jmp_next_removes_adjacent_forward_jump() {
        let mut bc = vec![Op::Jmp { target: 1 }, Op::Ret];
        crate::opt::bc::jmp_next(0, &mut bc);
        assert_eq!(bc, vec![Op::Nop, Op::Ret]);
    }

    #[test]
    fn jmp_next_removes_adjacent_forward_jump_at_nonzero_pos() {
        let mut window = vec![Op::Jmp { target: 6 }, Op::Ret];
        crate::opt::bc::jmp_next(5, &mut window);
        assert_eq!(window, vec![Op::Nop, Op::Ret]);
    }

    #[test]
    fn jmp_next_leaves_far_forward_jump() {
        let mut bc = vec![Op::Jmp { target: 5 }, Op::Ret];
        crate::opt::bc::jmp_next(0, &mut bc);
        assert_eq!(bc, vec![Op::Jmp { target: 5 }, Op::Ret]);
    }

    #[test]
    fn jmp_next_leaves_backward_jump() {
        let mut bc = vec![Op::Jmp { target: 0 }, Op::Ret];
        // pos=5 means the jump goes back 5 target is not pos+1.
        crate::opt::bc::jmp_next(5, &mut bc);
        assert_eq!(bc, vec![Op::Jmp { target: 0 }, Op::Ret]);
    }

    #[test]
    fn jmp_next_leaves_self_jump() {
        // Infinite loop (Jmp to self) is target == pos, not pos+1; must
        // not be Nop'd or we silently break the program's semantics.
        let mut bc = vec![Op::Jmp { target: 3 }, Op::Ret];
        crate::opt::bc::jmp_next(3, &mut bc);
        assert_eq!(bc, vec![Op::Jmp { target: 3 }, Op::Ret]);
    }

    #[test]
    fn jmp_next_leaves_conditional_jmpt() {
        // JmpT{cond, target: pos+1} is also semantically redundant (both
        // taken/fallthrough paths land at pos+1), but jmp_next only
        // matches unconditional Jmp. If extended later, flip this test
        // to assert Nop.
        let mut bc = vec![Op::JmpT { cond: 1, target: 1 }, Op::Ret];
        crate::opt::bc::jmp_next(0, &mut bc);
        assert_eq!(bc, vec![Op::JmpT { cond: 1, target: 1 }, Op::Ret]);
    }

    #[test]
    fn jmp_next_leaves_non_jmp_at_window_head() {
        let mut bc = vec![Op::Mov { dst: 0, src: 1 }, Op::Ret];
        crate::opt::bc::jmp_next(0, &mut bc);
        assert_eq!(bc, vec![Op::Mov { dst: 0, src: 1 }, Op::Ret]);
    }
}
