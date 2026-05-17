use crate::vm::op::Op;

/// imm_fold merges a LoadI into the immediately following binary op
/// when the loaded register is one of its operands.
///
/// Covers integer arith (IAdd/ISub/IMul/IDiv) and integer compares
/// (IEq/IGt/ILt).
///
/// ```text
/// LoadI { dst: 1, value: 0 }
/// IEq { dst: 2, lhs: 3, rhs: 1 }
/// ```
///
/// becomes
///
/// ```text
/// Nop
/// IEqI { dst: 2, lhs: 3, imm: 0 }
/// ```
///
/// Commutative ops (IEq, IAdd, IMul) fold from either side. For
/// non-commutative compares (IGt/ILt) the pass swaps to the
/// opposite op when the immediate sat on the lhs (for instance `imm > r[x]`
/// becomes ILtI { lhs: x, imm }). For non-commutative arith
/// (ISub/IDiv) we only fold the rhs-immediate form.
///
/// SAFETY: this pass NOPs the LoadI without checking downstream uses of load_dst. This relies on
/// the bc emitter only emitting LoadConst immediately before its single use. If a future codegen
/// path keeps the loaded constant live past the consumer this will silently miscompile; at which
/// point this should be hoisted to IR where SSA single-use is observable.
pub fn imm_fold(window: &mut [Op]) {
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
        // commutative: fold from either side
        Op::IEq { dst, lhs, rhs } => match commutative_lhs(load_dst, lhs, rhs) {
            Some(lhs) => Op::IEqI {
                dst,
                lhs,
                imm: value,
            },
            None => return,
        },
        Op::IAdd { dst, lhs, rhs } => match commutative_lhs(load_dst, lhs, rhs) {
            Some(lhs) => Op::IAddI {
                dst,
                lhs,
                imm: value,
            },
            None => return,
        },
        Op::IMul { dst, lhs, rhs } => match commutative_lhs(load_dst, lhs, rhs) {
            Some(lhs) => Op::IMulI {
                dst,
                lhs,
                imm: value,
            },
            None => return,
        },
        // non-commutative compares: swap to the opposite op when the
        // load sat on the lhs side.
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
        // non-commutative arith: only fold the rhs-immediate form.
        Op::ISub { dst, lhs, rhs } => {
            if load_dst == rhs {
                Op::ISubI {
                    dst,
                    lhs,
                    imm: value,
                }
            } else {
                return;
            }
        }
        Op::IDiv { dst, lhs, rhs } => {
            if load_dst == rhs {
                Op::IDivI {
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
    opt_trace!("imm_fold", "merged load_imm and bin op into imm bin op");
}

/// For a commutative op, return the non-immediate operand if the
/// loaded register matches either side, else None.
#[inline]
fn commutative_lhs(load_dst: u8, lhs: u8, rhs: u8) -> Option<u8> {
    if load_dst == lhs {
        Some(rhs)
    } else if load_dst == rhs {
        Some(lhs)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::imm_fold;
    use crate::vm::op::Op;

    #[test]
    fn folds_ieq() {
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 42 },
            Op::IEq {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ];
        imm_fold(&mut bc);
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
    fn folds_iadd_rhs() {
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 3 },
            Op::IAdd {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ];
        imm_fold(&mut bc);
        assert_eq!(
            bc,
            vec![
                Op::Nop,
                Op::IAddI {
                    dst: 2,
                    lhs: 0,
                    imm: 3,
                },
            ]
        )
    }

    #[test]
    fn folds_iadd_lhs_commutes() {
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 3 },
            Op::IAdd {
                dst: 2,
                lhs: 1,
                rhs: 0,
            },
        ];
        imm_fold(&mut bc);
        assert_eq!(
            bc,
            vec![
                Op::Nop,
                Op::IAddI {
                    dst: 2,
                    lhs: 0,
                    imm: 3,
                },
            ]
        )
    }

    #[test]
    fn folds_imul_either_side() {
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 5 },
            Op::IMul {
                dst: 2,
                lhs: 1,
                rhs: 0,
            },
        ];
        imm_fold(&mut bc);
        assert_eq!(
            bc,
            vec![
                Op::Nop,
                Op::IMulI {
                    dst: 2,
                    lhs: 0,
                    imm: 5,
                },
            ]
        )
    }

    #[test]
    fn folds_isub_rhs_only() {
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 4 },
            Op::ISub {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ];
        imm_fold(&mut bc);
        assert_eq!(
            bc,
            vec![
                Op::Nop,
                Op::ISubI {
                    dst: 2,
                    lhs: 0,
                    imm: 4,
                },
            ]
        )
    }

    #[test]
    fn leaves_isub_with_lhs_imm() {
        // No fold for `imm - r[x]`; we don't have a swap form for ISub.
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 4 },
            Op::ISub {
                dst: 2,
                lhs: 1,
                rhs: 0,
            },
        ];
        let snapshot = bc.clone();
        imm_fold(&mut bc);
        assert_eq!(bc, snapshot);
    }

    #[test]
    fn folds_idiv_rhs_only() {
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 2 },
            Op::IDiv {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ];
        imm_fold(&mut bc);
        assert_eq!(
            bc,
            vec![
                Op::Nop,
                Op::IDivI {
                    dst: 2,
                    lhs: 0,
                    imm: 2,
                },
            ]
        )
    }

    #[test]
    fn leaves_idiv_with_lhs_imm() {
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 2 },
            Op::IDiv {
                dst: 2,
                lhs: 1,
                rhs: 0,
            },
        ];
        let snapshot = bc.clone();
        imm_fold(&mut bc);
        assert_eq!(bc, snapshot);
    }

    #[test]
    fn folds_igt_rhs() {
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 7 },
            Op::IGt {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ];
        imm_fold(&mut bc);
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
    fn folds_igt_lhs_swaps_to_ilti() {
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 7 },
            Op::IGt {
                dst: 2,
                lhs: 1,
                rhs: 0,
            },
        ];
        imm_fold(&mut bc);
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
    fn folds_ilt_rhs() {
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 7 },
            Op::ILt {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ];
        imm_fold(&mut bc);
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
    fn folds_ilt_lhs_swaps_to_igti() {
        let mut bc = vec![
            Op::LoadI { dst: 1, value: 7 },
            Op::ILt {
                dst: 2,
                lhs: 1,
                rhs: 0,
            },
        ];
        imm_fold(&mut bc);
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
        let mut bc = vec![
            Op::LoadI { dst: 5, value: 7 },
            Op::IGt {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ];
        let snapshot = bc.clone();
        imm_fold(&mut bc);
        assert_eq!(bc, snapshot);
    }

    #[test]
    fn leaves_non_load_at_window_head() {
        let mut bc = vec![
            Op::Mov { dst: 1, src: 0 },
            Op::IAdd {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ];
        let snapshot = bc.clone();
        imm_fold(&mut bc);
        assert_eq!(bc, snapshot);
    }
}
