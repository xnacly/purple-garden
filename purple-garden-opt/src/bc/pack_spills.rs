use purple_garden_runtime::op::Op;

/// pack_spills merges adjacent spill-stack ops into fixed-width variants:
///
/// ```text
/// Push { src: 1 }
/// Push { src: 2 }
/// Push { src: 3 }
/// ```
///
/// Into
///
/// ```text
/// Push3 { a: 1, b: 2, c: 3 }
/// Nop
/// Nop
/// ```
///
/// same for 2 pushes and for pops.
pub fn pack_spills(bc: &mut [Op]) {
    match bc {
        [
            Op::Push { src: a },
            Op::Push { src: b },
            Op::Push { src: c },
            ..,
        ] => {
            bc[0] = Op::Push3 {
                a: *a,
                b: *b,
                c: *c,
            };
            bc[1] = Op::Nop;
            bc[2] = Op::Nop;
            opt_trace!("pack_spills", "packed three pushes");
        }
        [Op::Push { src: a }, Op::Push { src: b }, ..] => {
            bc[0] = Op::Push2 { a: *a, b: *b };
            bc[1] = Op::Nop;
            opt_trace!("pack_spills", "packed two pushes");
        }
        [
            Op::Pop { dst: a },
            Op::Pop { dst: b },
            Op::Pop { dst: c },
            ..,
        ] => {
            bc[0] = Op::Pop3 {
                a: *a,
                b: *b,
                c: *c,
            };
            bc[1] = Op::Nop;
            bc[2] = Op::Nop;
            opt_trace!("pack_spills", "packed three pops");
        }
        [Op::Pop { dst: a }, Op::Pop { dst: b }, ..] => {
            bc[0] = Op::Pop2 { a: *a, b: *b };
            bc[1] = Op::Nop;
            opt_trace!("pack_spills", "packed two pops");
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::pack_spills;
    use purple_garden_runtime::op::Op;

    #[test]
    fn merges_three_pushes_and_pops() {
        let mut bc = vec![
            Op::Push { src: 1 },
            Op::Push { src: 2 },
            Op::Push { src: 3 },
        ];

        pack_spills(&mut bc);
        assert_eq!(bc, vec![Op::Push3 { a: 1, b: 2, c: 3 }, Op::Nop, Op::Nop,]);
        let mut bc = vec![Op::Pop { dst: 4 }, Op::Pop { dst: 3 }, Op::Pop { dst: 2 }];
        pack_spills(&mut bc);
        assert_eq!(bc, vec![Op::Pop3 { a: 4, b: 3, c: 2 }, Op::Nop, Op::Nop,]);
    }
}
