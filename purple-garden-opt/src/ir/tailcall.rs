use purple_garden_ir::{self as ir, Instr};

/// Converts a call in tailcall position into a tailcall. See
/// [Wikipedia](https://en.wikipedia.org/wiki/Tail_call).
///
/// A call is defined as a tail call, if:
///
/// 1. its the last operation in a functions body
/// 2. its result is returned from the function
///
/// This means in purple gardens ir a tail call can be trivially detected by checking if a blocks
/// last instruction is a call and if said block terminates by jumping to the final block of the
/// function said block is from.
///
/// In ir terms, this detects both patterns A and B:
///
/// - A: very simple tailcalls:
///
/// ```text
/// // tailcallee
/// fn f1(%v0) -> Int {
/// b0(%v0):
///         ret %v0
/// }
///
/// // tailcall
/// fn f2(%v0) -> Int {
/// b0(%v0):
///         // tailcall, because %v1 is instantly returned
///         %v1:Int = call f1(%v0)
///         ret %v1
/// }
/// ```
///
/// - B: tailcalls spanning blocks
/// ```text
/// b3(%v0, %v1):
///         // tailcall, because the return flows to ret
///         %v7:Int = call f1(%v5, %v6)
///         jmp b4(%v7)
/// b4(%v7):
///         ret %v7
/// ```
pub fn tailcall(fun: &mut ir::Func) {
    let last_id = fun.blocks.len() - 1;

    // verify the return block is trivial: no instructions and a simple return
    let last = &fun.blocks[last_id];
    let trivial_return = matches!(
        (&last.instructions[..], &last.term),
        ([], Some(ir::Terminator::Return { .. }))
    );

    for i in 0..fun.blocks.len() {
        if fun.blocks[i].tombstone {
            continue;
        }

        // last instruction must be a call
        let Some(Instr::Call {
            dst,
            func,
            args,
            span,
        }) = fun.blocks[i].instructions.last().cloned()
        else {
            continue;
        };

        // Clone the term so we can hold the match guard's borrow without
        // blocking calls to fun.params(...) (a Pattern B check needs to
        // resolve the Jump's ParamsId through the function's pool).
        let term = fun.blocks[i].term.clone();

        let is_tail = match &term {
            // Pattern A: direct return
            Some(ir::Terminator::Return { value: Some(v), .. }) if v.0 == dst.id.0 => true,

            // Pattern B: jump to canonical return block
            Some(ir::Terminator::Jump {
                id: ir::Id(id),
                params: jump_params,
                ..
            }) if *id == last_id as u32 && trivial_return => {
                let resolved = fun.params(*jump_params);
                resolved.len() == 1 && resolved[0].0 == dst.id.0
            }

            _ => false,
        };

        if !is_tail {
            continue;
        }

        let block = &mut fun.blocks[i];

        opt_trace!(
            "ir::tailcall",
            format!("tailcalled b{}s last instruction", i)
        );

        block.instructions.pop();
        block.term = Some(ir::Terminator::Tail { func, args, span });
    }
}

#[cfg(test)]
mod tests {
    use super::tailcall;
    use purple_garden_ir::{self as ir, Block, Id, Instr, Terminator, TypeId, ptype::Type};

    /// Pattern A: a block whose last instruction is a Call followed by a
    /// Return of the call's dst must be rewritten to a Tail.
    #[test]
    fn rewrites_call_return_pattern_a() {
        let mut fun = ir::Func::new("tail", Id(0), vec![Id(0)], Some(Type::Int));
        let b0_params = fun.intern_params(vec![Id(0)]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params: b0_params,
            instructions: vec![Instr::Call {
                dst: TypeId {
                    id: Id(1),
                    ty: Type::Int,
                },
                func: Id(42),
                args: vec![Id(0)],
                span: 0,
            }],
            term: Some(Terminator::Return {
                value: Some(Id(1)),
                span: 0,
            }),
        }];

        tailcall(&mut fun);

        assert!(fun.blocks[0].instructions.is_empty());
        assert!(matches!(
            &fun.blocks[0].term,
            Some(Terminator::Tail { func, args, .. }) if *func == Id(42) && args == &vec![Id(0)]
        ));
    }

    /// Pattern B: a block that ends with Call then Jump to a trivial
    /// return block (passing the call's dst as the lone jump arg) is
    /// also a tail call.
    #[test]
    fn rewrites_call_jump_to_trivial_return_pattern_b() {
        let mut fun = ir::Func::new("tail", Id(0), vec![Id(0)], Some(Type::Int));
        let b0_params = fun.intern_params(vec![Id(0)]);
        let b1_params = fun.intern_params(vec![Id(2)]);
        let jump_params = fun.intern_params(vec![Id(1)]);
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params: b0_params,
                instructions: vec![Instr::Call {
                    dst: TypeId {
                        id: Id(1),
                        ty: Type::Int,
                    },
                    func: Id(42),
                    args: vec![Id(0)],
                    span: 0,
                }],
                term: Some(Terminator::Jump {
                    id: Id(1),
                    params: jump_params,
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params: b1_params,
                instructions: vec![],
                term: Some(Terminator::Return {
                    value: Some(Id(2)),
                    span: 0,
                }),
            },
        ];

        tailcall(&mut fun);

        assert!(fun.blocks[0].instructions.is_empty());
        assert!(matches!(
            &fun.blocks[0].term,
            Some(Terminator::Tail { func, .. }) if *func == Id(42)
        ));
    }

    /// A call whose result is not what the function returns is NOT a
    /// tail call and must be left in place.
    #[test]
    fn leaves_non_tail_call_alone() {
        let mut fun = ir::Func::new("nontail", Id(0), vec![Id(0)], Some(Type::Int));
        let b0_params = fun.intern_params(vec![Id(0)]);
        fun.blocks = vec![Block {
            tombstone: false,
            id: Id(0),
            params: b0_params,
            instructions: vec![Instr::Call {
                dst: TypeId {
                    id: Id(1),
                    ty: Type::Int,
                },
                func: Id(42),
                args: vec![Id(0)],
                span: 0,
            }],
            // Returns %v0, not the call's dst %v1.
            term: Some(Terminator::Return {
                value: Some(Id(0)),
                span: 0,
            }),
        }];

        tailcall(&mut fun);

        assert_eq!(fun.blocks[0].instructions.len(), 1);
        assert!(matches!(
            &fun.blocks[0].term,
            Some(Terminator::Return {
                value: Some(Id(0)),
                ..
            })
        ));
    }
}
