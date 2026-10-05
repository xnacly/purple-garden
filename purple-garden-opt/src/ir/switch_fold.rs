use purple_garden_ir as ir;

/// Chains shorter than this stay linear, a few compares beat a table lookup.
pub const MIN_CASES: usize = 4;

/// Fold a chain of equality branches on one subject into a single
/// [`ir::Terminator::Switch`]:
///
/// ```text
/// b1(%v0):                                        b1(%v0):
///     %v1:Str = `alpha`                               switch %v0 [`alpha` -> b2(%v0),
///     br_cmp SEq %v0, %v1, b2(%v0), b3(%v0)                       `beta` -> b4(%v0), ...], b7(%v0)
/// b3(%v0):                                   =>   b3(%v0):
///     %v4:Str = `beta`                                <tombstone>
///     br_cmp SEq %v0, %v4, b4(%v0), b5(%v0)
/// ...
/// ```
///
/// Integer chains (`br_imm IEq %v0, k, ...`) fold the same way. A block joins
/// the chain if it compares the same subject, holds nothing but the
/// single-use key `LoadConst` (nothing for integers), is only reached from
/// the previous chain block, and every edge passes its target's params
/// unchanged, so the switch needs no edge moves. A repeated key keeps its
/// first arm, as `match` takes the first one.
pub fn switch_fold(fun: &mut ir::Func<'_>, scratch: &mut super::Scratch<'_>) {
    // TODO: not implemented yet, the tests below describe the rewrite.
    let _ = (fun, scratch, MIN_CASES);
}

#[cfg(test)]
mod tests {
    use super::switch_fold;
    use purple_garden_ir::{
        BinOp, Block, Const, Func, Id, Instr, ParamsId, Terminator, TypeId, ptype::Type,
    };

    const V0: Id = Id(0);
    const V1: Id = Id(1);

    #[derive(Clone, Copy)]
    enum Key {
        Str(&'static str),
        Int(i32),
    }

    /// `match { subject == key { k + 1 } ... { 0 } }` as match lowering and
    /// branch_cmp leave it: chain block `b{2k}` compares case `k`, arm
    /// `b{2k+1}` returns `k + 1`, the last block `b{2n}` returns 0. Every block
    /// takes and passes the params `(%v0, %v1)`.
    fn chain(cases: &[(Id, Key)]) -> Func<'static> {
        let mut fun = Func::new("f", Id(0), vec![V0, V1], Some(Type::Int));
        let params = fun.intern_params(vec![V0, V1]);
        for (k, &(subject, key)) in cases.iter().enumerate() {
            let k = k as u32;
            let (yes, no) = ((Id(2 * k + 1), params), (Id(2 * k + 2), params));
            let (instructions, term) = match key {
                Key::Str(s) => (
                    vec![Instr::LoadConst {
                        dst: TypeId {
                            id: Id(100 + k),
                            ty: Type::Str,
                        },
                        value: Const::Str(s.into()),
                        span: 0,
                    }],
                    Terminator::BranchCmp {
                        op: BinOp::SEq,
                        lhs: subject,
                        rhs: Id(100 + k),
                        yes,
                        no,
                        span: 0,
                    },
                ),
                Key::Int(imm) => (
                    vec![],
                    Terminator::BranchCmpImm {
                        op: BinOp::IEq,
                        lhs: subject,
                        imm,
                        yes,
                        no,
                        span: 0,
                    },
                ),
            };
            fun.blocks.push(Block {
                tombstone: false,
                id: Id(2 * k),
                params,
                instructions,
                term: Some(term),
            });
            fun.blocks
                .push(arm(Id(2 * k + 1), i64::from(k) + 1, params));
        }
        fun.blocks.push(arm(Id(2 * cases.len() as u32), 0, params));
        fun
    }

    fn arm(id: Id, value: i64, params: ParamsId) -> Block<'static> {
        let dst = Id(200 + id.0);
        Block {
            tombstone: false,
            id,
            params,
            instructions: vec![Instr::LoadConst {
                dst: TypeId {
                    id: dst,
                    ty: Type::Int,
                },
                value: Const::Int(value),
                span: 0,
            }],
            term: Some(Terminator::Return {
                value: Some(dst),
                span: 0,
            }),
        }
    }

    fn fold(cases: &[(Id, Key)]) -> Func<'static> {
        let mut fun = chain(cases);
        switch_fold(&mut fun, &mut super::super::Scratch::default());
        fun
    }

    /// `(key, target)` of every case of `b0`'s switch on `%v0`, and its default.
    fn switch_of(fun: &Func<'static>) -> (Vec<(Const<'static>, Id)>, Id) {
        let Some(Terminator::Switch {
            subject: V0,
            cases,
            default,
            ..
        }) = &fun.blocks[0].term
        else {
            panic!("b0 is not a switch on %v0: {:?}", fun.blocks[0].term);
        };
        let cases = fun
            .cases(*cases)
            .iter()
            .map(|c| (c.key.clone(), c.target.0))
            .collect();
        (cases, default.0)
    }

    fn s(s: &'static str) -> Const<'static> {
        Const::Str(s.into())
    }

    #[test]
    #[ignore = "switch_fold is a stub"]
    fn folds_a_string_chain() {
        let fun = fold(&[
            (V0, Key::Str("alpha")),
            (V0, Key::Str("beta")),
            (V0, Key::Str("gamma")),
            (V0, Key::Str("delta")),
        ]);
        let (cases, default) = switch_of(&fun);
        assert_eq!(
            cases,
            [
                (s("alpha"), Id(1)),
                (s("beta"), Id(3)),
                (s("gamma"), Id(5)),
                (s("delta"), Id(7)),
            ]
        );
        assert_eq!(default, Id(8));
        // The head's key load is dead once the compare is gone.
        assert!(
            fun.blocks[0]
                .instructions
                .iter()
                .all(|i| matches!(i, Instr::Noop))
        );
        for id in [2, 4, 6] {
            assert!(fun.blocks[id].tombstone, "chain block b{id}");
        }
        for id in [1, 3, 5, 7, 8] {
            assert!(!fun.blocks[id].tombstone, "arm b{id}");
        }
    }

    #[test]
    #[ignore = "switch_fold is a stub"]
    fn folds_an_int_chain() {
        let fun = fold(&[
            (V0, Key::Int(3)),
            (V0, Key::Int(7)),
            (V0, Key::Int(9)),
            (V0, Key::Int(11)),
        ]);
        let (cases, default) = switch_of(&fun);
        assert_eq!(
            cases,
            [
                (Const::Int(3), Id(1)),
                (Const::Int(7), Id(3)),
                (Const::Int(9), Id(5)),
                (Const::Int(11), Id(7)),
            ]
        );
        assert_eq!(default, Id(8));
        for id in [2, 4, 6] {
            assert!(fun.blocks[id].tombstone, "chain block b{id}");
        }
    }

    #[test]
    fn leaves_a_chain_shorter_than_min_cases() {
        let fun = fold(&[
            (V0, Key::Str("alpha")),
            (V0, Key::Str("beta")),
            (V0, Key::Str("gamma")),
        ]);
        assert!(matches!(
            fun.blocks[0].term,
            Some(Terminator::BranchCmp { .. })
        ));
        assert!(fun.blocks.iter().all(|b| !b.tombstone));
    }

    #[test]
    #[ignore = "switch_fold is a stub"]
    fn first_arm_wins_for_a_repeated_key() {
        let fun = fold(&[
            (V0, Key::Str("alpha")),
            (V0, Key::Str("beta")),
            (V0, Key::Str("alpha")),
            (V0, Key::Str("gamma")),
            (V0, Key::Str("delta")),
        ]);
        let (cases, default) = switch_of(&fun);
        assert_eq!(
            cases,
            [
                (s("alpha"), Id(1)),
                (s("beta"), Id(3)),
                (s("gamma"), Id(7)),
                (s("delta"), Id(9)),
            ]
        );
        assert_eq!(default, Id(10));
    }

    #[test]
    #[ignore = "switch_fold is a stub"]
    fn chain_stops_at_a_different_subject() {
        let fun = fold(&[
            (V0, Key::Str("alpha")),
            (V0, Key::Str("beta")),
            (V0, Key::Str("gamma")),
            (V0, Key::Str("delta")),
            (V1, Key::Str("epsilon")),
        ]);
        let (cases, default) = switch_of(&fun);
        assert_eq!(cases.len(), 4);
        // The compare on %v1 becomes the default and stays as it is.
        assert_eq!(default, Id(8));
        assert!(!fun.blocks[8].tombstone);
        assert!(matches!(
            fun.blocks[8].term,
            Some(Terminator::BranchCmp { lhs: V1, .. })
        ));
    }
}
