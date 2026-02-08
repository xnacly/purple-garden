mod const_binary;
mod self_move;

#[allow(unused)]
pub use const_binary::const_binary;
pub use self_move::self_move;

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
    fn const_binary() {
        let mut bc = vec![
            Op::LoadI { dst: 0, value: 1 },
            Op::LoadI { dst: 1, value: 2 },
            Op::IAdd {
                dst: 0,
                lhs: 0,
                rhs: 1,
            },
            Op::LoadI { dst: 0, value: 1 },
            Op::LoadI { dst: 1, value: 2 },
            Op::ISub {
                dst: 0,
                lhs: 0,
                rhs: 1,
            },
            Op::LoadI { dst: 0, value: 3 },
            Op::LoadI { dst: 1, value: 5 },
            Op::IMul {
                dst: 0,
                lhs: 0,
                rhs: 1,
            },
            Op::LoadI { dst: 0, value: 64 },
            Op::LoadI { dst: 1, value: 8 },
            Op::IDiv {
                dst: 0,
                lhs: 0,
                rhs: 1,
            },
        ];

        for i in 0..=bc.len().saturating_sub(3) {
            crate::opt::bc::const_binary(&mut bc[i..i + 3]);
        }

        bc.retain(|op| *op != Op::Nop);
        assert_eq!(
            bc,
            vec![
                Op::LoadI { dst: 0, value: 1 },
                Op::LoadI { dst: 1, value: 2 },
                Op::LoadI { dst: 0, value: 3 },
                //
                Op::LoadI { dst: 0, value: 1 },
                Op::LoadI { dst: 1, value: 2 },
                Op::LoadI { dst: 0, value: -1 },
                //
                Op::LoadI { dst: 0, value: 3 },
                Op::LoadI { dst: 1, value: 5 },
                Op::LoadI { dst: 0, value: 15 },
                //
                Op::LoadI { dst: 0, value: 64 },
                Op::LoadI { dst: 1, value: 8 },
                Op::LoadI { dst: 0, value: 8 },
            ]
        )
    }
}
