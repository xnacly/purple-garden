use std::alloc::Allocator;

use purple_garden_ir::{Const, Func, Instr};

/// Replaces calls to constant functions (see [`super::is_const_call`] for the matching detection) with the constant they
/// return and resolve toooo
///
/// ```text
/// %v1:Double = Call f1()
/// ```
///
/// Where `f1` is `fn PI() Double { 3.1415 }`, folded into:
///
/// ```text
/// %v1:Double = 3.1415
/// ```
pub fn const_fold_const_calls<'f, F: Allocator>(
    fun: &mut Func<'f, F>,
    consts: &[Option<Const<'f>>],
) {
    for block in &mut fun.blocks {
        if block.tombstone {
            continue;
        }

        for instr in &mut block.instructions {
            let Instr::Call {
                dst, func, span, ..
            } = instr
            else {
                continue;
            };
            let Some(value) = consts.get(func.0 as usize).and_then(Option::as_ref) else {
                continue;
            };

            purple_garden_shared::trace!(
                "[opt::ir::const_fold_const_calls] folded call to f{} into constant {:?}",
                func.0,
                value
            );
            *instr = Instr::LoadConst {
                dst: dst.clone(),
                value: value.clone(),
                span: *span,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::const_fold_const_calls;
    use crate::ir::{Scratch, const_fold, is_const_call};
    use purple_garden_ir::{
        BinOp, Block, EMPTY_PARAMS, Func, Id, Instr, Terminator, TypeId, constant::Const,
        ptype::Type,
    };

    fn double(id: u32) -> TypeId<'static> {
        TypeId {
            id: Id(id),
            ty: Type::Double,
        }
    }

    fn func(id: u32, instructions: Vec<Instr<'static>>, ret: u32) -> Func<'static> {
        let mut fun = Func::new("f", Id(id), vec![], Some(Type::Double));
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            instructions,
            params: EMPTY_PARAMS,
            term: Some(Terminator::Return {
                value: Some(Id(ret)),
                span: 0,
            }),
        }];
        fun
    }

    fn call(dst: u32, callee: u32) -> Instr<'static> {
        Instr::Call {
            dst: double(dst),
            func: Id(callee),
            args: EMPTY_PARAMS,
            span: 0,
        }
    }

    fn load(dst: u32, value: f64) -> Instr<'static> {
        Instr::LoadConst {
            dst: double(dst),
            value: Const::Double(value.to_bits()),
            span: 0,
        }
    }

    #[test]
    fn replaces_only_calls_to_constant_functions() {
        let consts = [None, Some(Const::Double(3.1415f64.to_bits())), None];
        let mut fun = func(3, vec![call(0, 1), call(1, 2)], 1);

        const_fold_const_calls(&mut fun, &consts);

        let instructions = &fun.blocks[0].instructions;
        assert!(matches!(
            &instructions[0],
            Instr::LoadConst { dst: TypeId { id: Id(0), ty: Type::Double }, value: Const::Double(bits), .. }
                if *bits == 3.1415f64.to_bits()
        ));
        assert!(matches!(instructions[1], Instr::Call { func: Id(2), .. }));
    }

    /// `fn TAU() Double { PI() * 2.0 }` only becomes constant once the call to PI is folded.
    #[test]
    fn folded_call_makes_caller_constant() {
        let consts = [None, Some(Const::Double(3.1415f64.to_bits()))];
        let mut tau = func(
            2,
            vec![
                call(0, 1),
                load(1, 2.0),
                Instr::Bin {
                    op: BinOp::DMul,
                    dst: double(2),
                    lhs: Id(0),
                    rhs: Id(1),
                    span: 0,
                },
            ],
            2,
        );
        assert_eq!(is_const_call(&tau), None);

        const_fold_const_calls(&mut tau, &consts);
        const_fold(&mut tau, &mut Scratch::default());

        assert_eq!(
            is_const_call(&tau),
            Some(Const::Double((3.1415f64 * 2.0).to_bits()))
        );
    }
}
