use std::alloc::Allocator;

use purple_garden_ir::{self as ir, BinOp, Case, Const, Id, Instr, ParamsId, Terminator};

/// Chains shorter than this stay linear, a few compares beat a table lookup.
// PERF: benchmark this claim
pub const MIN_CASES: usize = 4;

/// A switch's jump table has a slot for every value between its lowest and
/// highest key. Integer chains spread wider than this per case would leave it
/// mostly holes and stay compare chains.
pub const MAX_SLOTS_PER_CASE: usize = 4;

/// Fold a chain of equality branches on one subject into a single
/// [`ir::Terminator::Switch`]:
///
/// ```text
/// b1(%v0):                                        b1(%v0):
///     %v1:Str = `alpha`                               switch %v0
///     br_cmp SEq %v0, %v1, b2(%v0), b3(%v0)               `alpha` -> b2(%v0)
/// b3(%v0):                                   =>           `beta`  -> b4(%v0)
///     %v4:Str = `beta`                                    ...
///     br_cmp SEq %v0, %v4, b4(%v0), b5(%v0)               _       -> b7(%v0)
/// ...                                             b3(%v0):
///                                                     <tombstone>
/// ```
///
/// Integer chains (`br_imm IEq %v0, k, ...`) fold the same way
pub fn switch_fold<F: Allocator, S: Allocator>(
    fun: &mut ir::Func<'_, F>,
    scratch: &mut super::Scratch<'_, S>,
) {
    super::record_uses(fun, scratch);
    let alloc = scratch.alloc();
    let preds = super::predecessor_counts(fun, alloc);

    for head in 0..fun.blocks.len() {
        if fun.blocks[head].tombstone {
            continue;
        }
        let Some((subject, key, head_key, yes, mut default)) = compare(fun, head) else {
            continue;
        };

        let mut cases = vec![Case { key, target: yes }];
        let mut chain = Vec::new_in(alloc);
        loop {
            let b = default.0.0 as usize;
            // The block is dropped whole, so only the previous chain block may lead here.
            if b == head || preds[b] != 1 {
                break;
            }
            let Some((lhs, key, key_load, yes, no)) = compare(fun, b) else {
                break;
            };
            // Besides its key, a chain block may only hold constants imm_fold left dead.
            let only_constants = fun.blocks[b].instructions.iter().all(|instr| match instr {
                Instr::Noop => true,
                Instr::LoadConst { dst, .. } => {
                    Some(dst.id) == key_load || scratch.use_count(dst.id) == 0
                }
                _ => false,
            });
            if lhs != subject || !only_constants {
                break;
            }
            if !cases.iter().any(|case| case.key == key) {
                cases.push(Case { key, target: yes });
            }
            chain.push(b);
            default = no;
        }

        if cases.len() < MIN_CASES {
            continue;
        }
        let mut ints = Vec::new_in(alloc);
        ints.extend(cases.iter().filter_map(|case| match case.key {
            Const::Int(v) => Some(v),
            _ => None,
        }));
        if let (Some(min), Some(max)) = (ints.iter().min(), ints.iter().max())
            && (max - min) as usize >= MAX_SLOTS_PER_CASE * cases.len()
        {
            continue;
        }

        purple_garden_shared::trace!(
            "[opt::ir::switch_fold] b{head} switches over {} cases",
            cases.len()
        );

        for b in chain {
            fun.blocks[b].tombstone = true;
        }
        let cases = fun.intern_cases(cases);
        let block = &mut fun.blocks[head];
        if let Some(key) = head_key
            && scratch.use_count(key) == 1
        {
            for instr in &mut block.instructions {
                if ir::Func::def_of(instr) == Some(key) {
                    *instr = Instr::Noop;
                }
            }
        }
        let span = block.term.as_ref().map_or(0, Terminator::span);
        block.term = Some(Terminator::Switch {
            subject,
            cases,
            default,
            span,
        });
    }
}

type Edge = (Id, ParamsId);

/// `(subject, key, key_load, yes, no)` of a `subject == key` branch. `key_load`
/// is the `LoadConst` of a string key, integer keys are immediates. A switch
/// moves nothing along its edges, so both must pass their target's params
/// unchanged.
fn compare<'f, F: Allocator>(
    fun: &ir::Func<'f, F>,
    b: usize,
) -> Option<(Id, Const<'f>, Option<Id>, Edge, Edge)> {
    let block = &fun.blocks[b];
    let (subject, key, key_load, yes, no) = match block.term {
        Some(Terminator::BranchCmp {
            op: BinOp::SEq,
            lhs,
            rhs,
            yes,
            no,
            ..
        }) => {
            let key = block.instructions.iter().find_map(|instr| match instr {
                Instr::LoadConst {
                    dst,
                    value: value @ Const::Str(_),
                    ..
                } if dst.id == rhs => Some(value.clone()),
                _ => None,
            })?;
            (lhs, key, Some(rhs), yes, no)
        }
        Some(Terminator::BranchCmpImm {
            op: BinOp::IEq,
            lhs,
            imm,
            yes,
            no,
            ..
        }) => (lhs, Const::Int(i64::from(imm)), None, yes, no),
        _ => return None,
    };

    let unchanged = |(target, params): Edge| {
        fun.params(params) == fun.params(fun.blocks[target.0 as usize].params)
    };
    (unchanged(yes) && unchanged(no)).then_some((subject, key, key_load, yes, no))
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
                // imm_fold leaves the key's LoadConst dead for dce
                Key::Int(imm) => (
                    vec![Instr::LoadConst {
                        dst: TypeId {
                            id: Id(100 + k),
                            ty: Type::Int,
                        },
                        value: Const::Int(i64::from(imm)),
                        span: 0,
                    }],
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
    fn leaves_a_sparse_int_chain() {
        let fun = fold(&[
            (V0, Key::Int(3)),
            (V0, Key::Int(7)),
            (V0, Key::Int(9)),
            (V0, Key::Int(42)),
        ]);
        assert!(matches!(
            fun.blocks[0].term,
            Some(Terminator::BranchCmpImm { .. })
        ));
        assert!(fun.blocks.iter().all(|b| !b.tombstone));
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
