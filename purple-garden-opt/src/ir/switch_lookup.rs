use std::alloc::Allocator;

use purple_garden_ir::{self as ir, Const, Id, Instr, Terminator, TypeId};

/// Turn a switch whose arms, the default included, only return a constant into
/// a lookup of that constant, so there is nothing left to branch on:
///
/// ```text
/// b0(%v0):                                b0(%v0):
///     switch %v0                              %v9 = Lookup %v0 [`a` -> 1, `b` -> 2], 0
///         `a` -> b1(%v0)                      ret %v9
///         `b` -> b2(%v0)          =>      b1(%v0):
///         _   -> b3(%v0)                      <tombstone>
/// b1(%v0):                                ...
///     %v3:Int = 1
///     ret %v3
/// ...
/// ```
pub fn switch_lookup<S: Allocator>(fun: &mut ir::Func<'_>, scratch: &mut super::Scratch<'_, S>) {
    let alloc = scratch.alloc();
    let preds = super::predecessor_counts(fun, alloc);

    for head in 0..fun.blocks.len() {
        if fun.blocks[head].tombstone {
            continue;
        }
        let Some(Terminator::Switch {
            subject,
            cases,
            default,
            span,
        }) = fun.blocks[head].term
        else {
            continue;
        };

        let mut arms = Vec::new_in(alloc);
        arms.extend(
            fun.cases(cases)
                .iter()
                .map(|case| case.target.0)
                .chain([default.0]),
        );
        let Some(mut returns) = super::try_collect_in(
            arms.iter().map(|&arm| returned_const(fun, arm, &preds)),
            alloc,
        ) else {
            continue;
        };

        // The default arm is dropped, its value id now names the lookup.
        let (dst, default) = returns.pop().unwrap();
        let entries = fun
            .cases(cases)
            .iter()
            .zip(returns)
            .map(|(case, (_, value))| (case.key.clone(), value))
            .collect();

        purple_garden_shared::trace!("[opt::ir::switch_lookup] b{head} looks up its result");

        for arm in arms {
            fun.blocks[arm.0 as usize].tombstone = true;
        }
        let block = &mut fun.blocks[head];
        block.instructions.push(Instr::Lookup {
            dst: dst.clone(),
            subject,
            entries,
            default,
            span,
        });
        block.term = Some(Terminator::Return {
            value: Some(dst.id),
            span,
        });
    }
}

/// `(dst, value)` of an arm only the switch leads to, that loads a constant and
/// returns it.
fn returned_const<'f>(
    fun: &ir::Func<'f>,
    arm: Id,
    preds: &[u32],
) -> Option<(TypeId<'f>, Const<'f>)> {
    let block = &fun.blocks[arm.0 as usize];
    if preds[arm.0 as usize] != 1 {
        return None;
    }
    let mut instrs = block
        .instructions
        .iter()
        .filter(|instr| !matches!(instr, Instr::Noop));
    let (Some(Instr::LoadConst { dst, value, .. }), None) = (instrs.next(), instrs.next()) else {
        return None;
    };
    match block.term {
        Some(Terminator::Return {
            value: Some(ret), ..
        }) if ret == dst.id => Some((dst.clone(), value.clone())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::switch_lookup;
    use purple_garden_ir::{Block, Case, Const, Func, Id, Instr, Terminator, TypeId, ptype::Type};

    const V0: Id = Id(0);

    fn load(id: u32, value: i64) -> Instr<'static> {
        Instr::LoadConst {
            dst: TypeId {
                id: Id(id),
                ty: Type::Int,
            },
            value: Const::Int(value),
            span: 0,
        }
    }

    /// `switch %v0` over `keys`: case `k` goes to arm `b{k+1}` returning `k+1`,
    /// the default arm returns 0.
    fn switch(keys: &[i64]) -> Func<'static> {
        let mut fun = Func::new("f", Id(0), vec![V0], Some(Type::Int));
        let params = fun.intern_params(vec![V0]);
        let cases = (1..)
            .zip(keys)
            .map(|(arm, &key)| Case {
                key: Const::Int(key),
                target: (Id(arm), params),
            })
            .collect();
        let cases = fun.intern_cases(cases);
        let default = keys.len() as u32 + 1;
        fun.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![],
            term: Some(Terminator::Switch {
                subject: V0,
                cases,
                default: (Id(default), params),
                span: 0,
            }),
        });
        for arm in 1..=default {
            let value = if arm == default { 0 } else { i64::from(arm) };
            fun.blocks.push(Block {
                tombstone: false,
                id: Id(arm),
                params,
                instructions: vec![load(100 + arm, value)],
                term: Some(Terminator::Return {
                    value: Some(Id(100 + arm)),
                    span: 0,
                }),
            });
        }
        fun
    }

    #[test]
    fn looks_up_arms_returning_constants() {
        let mut fun = switch(&[3, 7, 9]);
        switch_lookup(&mut fun, &mut super::super::Scratch::default());

        let Some(Instr::Lookup {
            dst,
            subject: V0,
            entries,
            default: Const::Int(0),
            ..
        }) = fun.blocks[0].instructions.last()
        else {
            panic!("b0 doesn't look up: {:?}", fun.blocks[0].instructions);
        };
        // The default arm's value id now names the lookup.
        assert_eq!(dst.id, Id(104));
        assert_eq!(
            &**entries,
            [
                (Const::Int(3), Const::Int(1)),
                (Const::Int(7), Const::Int(2)),
                (Const::Int(9), Const::Int(3)),
            ]
        );
        assert!(matches!(
            fun.blocks[0].term,
            Some(Terminator::Return {
                value: Some(Id(104)),
                ..
            })
        ));
        assert!(fun.blocks[1..].iter().all(|b| b.tombstone));
    }

    #[test]
    fn leaves_an_arm_doing_more_than_returning_a_constant() {
        let mut fun = switch(&[3, 7, 9]);
        fun.blocks[2].instructions.insert(0, load(200, 42));
        switch_lookup(&mut fun, &mut super::super::Scratch::default());

        assert!(matches!(
            fun.blocks[0].term,
            Some(Terminator::Switch { .. })
        ));
        assert!(fun.blocks.iter().all(|b| !b.tombstone));
    }
}
