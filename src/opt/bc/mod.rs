mod const_add;
mod self_move;

pub use const_add::const_add;
pub use self_move::self_move;

#[cfg(test)]
mod bc {
    use crate::op::Op;

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
    fn const_add() {
        let mut bc = vec![
            Op::LoadImm { dst: 0, value: 1 },
            Op::LoadImm { dst: 1, value: 2 },
            Op::Add {
                dst: 0,
                lhs: 0,
                rhs: 1,
            },
        ];
        crate::opt::bc::const_add(&mut bc);
        assert_eq!(
            bc,
            vec![Op::LoadImm { dst: 0, value: 3 }, Op::Nop, Op::Nop,]
        )
    }
}
