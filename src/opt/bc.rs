use crate::vm::op::Op;

/// self_move removes patterns conforming to
///
///     Mov { dst: x, src: x },
///
/// where both dst == src
pub fn self_move(window: &mut [Op]) {
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
///     Mov { dst: 8, src: 0 }
///     Mov { dst: 2, src: 8 }
///
/// Into
///
///     Mov { dst: 2, src: 0 }
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

    let (dst, src) = (*m1dst, *m0src);
    window[0] = Op::Nop;
    window[1] = Op::Mov { dst, src };
    opt_trace!("mov_merge", "merged two movs");
}

#[cfg(test)]
mod bc {
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
        crate::opt::bc::self_move(&mut bc);
        assert_eq!(bc, vec![Op::Nop, Op::Mov { dst: 2, src: 0 }])
    }
}
