use crate::vm::op::Op;

/// cmp_imm merges a load_imm into an integer comparison using the loaded
/// register as one of its operands:
///
/// ```text
/// LoadI { dst: 1, value: 0 }
/// IEq { dst: 2, lhs: 3, rhs: 1 }
/// ```
///
/// Into
///
/// ```text
/// Nop
/// IEqI { dst: 2, lhs: 3, imm: 0 }
/// ```
///
/// IGt and ILt fold the same way, but since /* arent commutative
/// the pass swaps the op when the constant sat on the lhs side:
///
/// imm > r[x] becomes ILtI { lhs: x, imm }, and imm < r[x] becomes
/// IGtI { lhs: x, imm }.
pub fn cmp_imm(window: &mut [Op]) {
    if window.len() < 2 {
        return;
    }
    let first = window[0];
    let second = window[1];

    let Op::LoadI {
        dst: load_dst,
        value,
    } = first
    else {
        return;
    };

    let folded = match second {
        Op::IEq { dst, lhs, rhs } => {
            let lhs = if load_dst == lhs {
                rhs
            } else if load_dst == rhs {
                lhs
            } else {
                return;
            };
            Op::IEqI {
                dst,
                lhs,
                imm: value,
            }
        }
        Op::IGt { dst, lhs, rhs } => {
            if load_dst == lhs {
                // imm > r[rhs] <=> r[rhs] < imm
                Op::ILtI {
                    dst,
                    lhs: rhs,
                    imm: value,
                }
            } else if load_dst == rhs {
                Op::IGtI {
                    dst,
                    lhs,
                    imm: value,
                }
            } else {
                return;
            }
        }
        Op::ILt { dst, lhs, rhs } => {
            if load_dst == lhs {
                // imm < r[rhs] <=> r[rhs] > imm
                Op::IGtI {
                    dst,
                    lhs: rhs,
                    imm: value,
                }
            } else if load_dst == rhs {
                Op::ILtI {
                    dst,
                    lhs,
                    imm: value,
                }
            } else {
                return;
            }
        }
        _ => return,
    };

    window[0] = Op::Nop;
    window[1] = folded;
    opt_trace!("cmp_imm", "merged load_imm into compare");
}

#[cfg(test)]
mod tests {
    use super::cmp_imm;
    use crate::vm::op::Op;

    #[test]
    fn merges_load_imm_into_ieq() {
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 42 },
            Op::IEq {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ];
        cmp_imm(&mut bc);
        assert_eq!(
            bc,
            vec![
                Op::Nop,
                Op::IEqI {
                    dst: 2,
                    lhs: 0,
                    imm: 42,
                },
            ]
        )
    }

    #[test]
    fn merges_load_imm_into_igt_rhs() {
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 7 },
            Op::IGt {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ];
        cmp_imm(&mut bc);
        assert_eq!(
            bc,
            vec![
                Op::Nop,
                Op::IGtI {
                    dst: 2,
                    lhs: 0,
                    imm: 7,
                },
            ]
        )
    }

    #[test]
    fn merges_load_imm_into_igt_lhs_swaps_to_ilti() {
        // load on lhs side: imm > r[rhs]  <=>  r[rhs] < imm
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 7 },
            Op::IGt {
                dst: 2,
                lhs: 1,
                rhs: 0,
            },
        ];
        cmp_imm(&mut bc);
        assert_eq!(
            bc,
            vec![
                Op::Nop,
                Op::ILtI {
                    dst: 2,
                    lhs: 0,
                    imm: 7,
                },
            ]
        )
    }

    #[test]
    fn merges_load_imm_into_ilt_rhs() {
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 7 },
            Op::ILt {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ];
        cmp_imm(&mut bc);
        assert_eq!(
            bc,
            vec![
                Op::Nop,
                Op::ILtI {
                    dst: 2,
                    lhs: 0,
                    imm: 7,
                },
            ]
        )
    }

    #[test]
    fn merges_load_imm_into_ilt_lhs_swaps_to_igti() {
        // load on lhs side: imm < r[rhs]  <=>  r[rhs] > imm
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 7 },
            Op::ILt {
                dst: 2,
                lhs: 1,
                rhs: 0,
            },
        ];
        cmp_imm(&mut bc);
        assert_eq!(
            bc,
            vec![
                Op::Nop,
                Op::IGtI {
                    dst: 2,
                    lhs: 0,
                    imm: 7,
                },
            ]
        )
    }

    #[test]
    fn leaves_unrelated_loads() {
        // load_dst doesn't appear in the compare operands
        let mut bc = vec![
            Op::LoadI { dst: 5, value: 7 },
            Op::IGt {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ];
        let snapshot = bc.clone();
        cmp_imm(&mut bc);
        assert_eq!(bc, snapshot);
    }
}
