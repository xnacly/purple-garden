use purple_garden_runtime::op::Op;

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
/// Nop
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

    window[0] = Op::Mov { dst, src };
    window[1] = Op::Nop;
    opt_trace!("mov_merge", "merged two movs");
}

#[cfg(test)]
mod tests {
    use super::mov_merge;
    use purple_garden_runtime::op::Op;

    #[test]
    fn merges_chained_movs() {
        let mut bc = vec![Op::Mov { dst: 8, src: 0 }, Op::Mov { dst: 2, src: 8 }];
        mov_merge(&mut bc);
        assert_eq!(bc, vec![Op::Mov { dst: 2, src: 0 }, Op::Nop])
    }

    #[test]
    fn leaves_non_mergable() {
        let mut bc = vec![Op::Mov { dst: 7, src: 0 }, Op::Mov { dst: 2, src: 8 }];
        mov_merge(&mut bc);
        assert_eq!(
            bc,
            vec![Op::Mov { dst: 7, src: 0 }, Op::Mov { dst: 2, src: 8 }]
        )
    }
}
