use std::alloc::Allocator;

use purple_garden_ir::{Const, Func, Instr, Terminator};

/// Returns the Const `fun` always evaluates to, for calls to it could be replaced by said constant
///
/// A function is considered to be constant if it consists of a single block holding only constant loads and
/// returning one of them:
///
/// ```text
/// // PI
/// fn f1() -> Double {
/// b0():
///         %v0:Double = 3.1415
///         ret %v0
/// }
/// ```
///
/// Therefore `%v1:Double = Call f1()` at its every call site can become `%v1:Double = 3.1415`:
///
/// ```text
/// // TAU, calling PI() and after constant folding merged `PI() * 2.0`
/// // due to const folding PI()
/// fn f2() -> Double {
/// b0():
///         %v2:Double = 6.283
///         ret %v2
/// }
/// ```
pub fn is_const_call<'f, F: Allocator>(fun: &Func<'f, F>) -> Option<Const<'f>> {
    let mut live = fun.blocks.iter().filter(|b| !b.tombstone);
    let (Some(block), None) = (live.next(), live.next()) else {
        return None;
    };

    let Some(Terminator::Return {
        value: Some(ret), ..
    }) = block.term
    else {
        return None;
    };

    // block should only have constants, since the rest has side effects we would have dropped by
    // folding them at call sites
    if !block
        .instructions
        .iter()
        .all(|i| matches!(i, Instr::LoadConst { .. } | Instr::Noop))
    {
        return None;
    }

    block.instructions.iter().find_map(|i| match i {
        Instr::LoadConst { dst, value, .. } if dst.id == ret => Some(value.clone()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::is_const_call;
    use purple_garden_ir::{
        Block, EMPTY_PARAMS, Func, Id, Instr, Terminator, TypeId, constant::Const, ptype::Type,
    };

    fn int(id: u32) -> TypeId<'static> {
        TypeId {
            id: Id(id),
            ty: Type::Int,
        }
    }

    fn block(id: u32, tombstone: bool, instructions: Vec<Instr<'static>>) -> Block<'static> {
        Block {
            tombstone,
            id: Id(id),
            instructions,
            params: EMPTY_PARAMS,
            term: (!tombstone).then_some(Terminator::Return {
                value: Some(Id(1)),
                span: 0,
            }),
        }
    }

    fn func(blocks: Vec<Block<'static>>) -> Func<'static> {
        let mut fun = Func::new("f", Id(1), vec![], Some(Type::Int));
        fun.blocks = blocks;
        fun
    }

    fn load(dst: u32, value: i64) -> Instr<'static> {
        Instr::LoadConst {
            dst: int(dst),
            value: Const::Int(value),
            span: 0,
        }
    }

    #[test]
    fn returns_the_returned_constant_ignoring_dead_ones_and_tombstones() {
        let fun = func(vec![
            block(0, false, vec![load(0, 1), Instr::Noop, load(1, 2)]),
            block(1, true, vec![]),
        ]);
        assert_eq!(is_const_call(&fun), Some(Const::Int(2)));
    }

    #[test]
    fn rejects_constant_return_next_to_side_effects() {
        let fun = func(vec![block(
            0,
            false,
            vec![
                Instr::Call {
                    dst: int(0),
                    func: Id(2),
                    args: EMPTY_PARAMS,
                    span: 0,
                },
                load(1, 2),
            ],
        )]);
        assert_eq!(is_const_call(&fun), None);
    }

    #[test]
    fn rejects_multiple_live_blocks() {
        let fun = func(vec![
            block(0, false, vec![load(1, 2)]),
            block(1, false, vec![load(1, 2)]),
        ]);
        assert_eq!(is_const_call(&fun), None);
    }
}
