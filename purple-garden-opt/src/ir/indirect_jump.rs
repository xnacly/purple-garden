use purple_garden_ir as ir;

/// merges a br to block A indirected by a single jmp in block B into a direct br to A:
///
/// ```text
///     b1:
///             %v4:Bool = True
///             br %v4, b2, b3
///     b2:
///             jmp b4(%v1) // <- redundant
///     b3:
///     b4:
/// ```
///
/// Should be:
/// ```text
///     b1:
///             %v4:Bool = True
///             br %v4, b4, b3
///     b3:
///     b4:
/// ```
pub fn indirect_jump(fun: &mut ir::Func) {
    for i in 0..fun.blocks.len() {
        let Some(ir::Terminator::Branch {
            cond,
            yes: (ir::Id(yes), yes_params),
            no: (ir::Id(no), no_params),
            span,
        }) = fun.blocks[i].term.clone()
        else {
            continue;
        };

        let yes_target = &mut fun.blocks[yes as usize];
        let yes_edge = if yes_target.instructions.is_empty() {
            if let Some(ir::Terminator::Jump { id, params, .. }) = &yes_target.term {
                yes_target.tombstone = true;
                Some((*id, *params))
            } else {
                None
            }
        } else {
            None
        };

        let no_target = &mut fun.blocks[no as usize];
        let no_edge = if no_target.instructions.is_empty() {
            if let Some(ir::Terminator::Jump { id, params, .. }) = &no_target.term {
                no_target.tombstone = true;
                Some((*id, *params))
            } else {
                None
            }
        } else {
            None
        };

        #[cfg(feature = "trace")]
        if yes_edge.is_some() {
            purple_garden_shared::trace!("[opt::ir::indirect_jump] b{yes} is now a tombstone");
        }

        #[cfg(feature = "trace")]
        if no_edge.is_some() {
            purple_garden_shared::trace!("[opt::ir::indirect_jump] b{no} is now a tombstone");
        }

        fun.blocks[i].term = Some(ir::Terminator::Branch {
            cond,
            yes: yes_edge.unwrap_or((ir::Id(yes), yes_params)),
            no: no_edge.unwrap_or((ir::Id(no), no_params)),
            span,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::indirect_jump;
    use purple_garden_ir::{self as ir, Block, Id, Terminator, ptype::Type};

    /// Build a function where b0 branches to b1 (which jumps to b3) and
    /// b2 (which does real work). After `indirect_jump`, b0 should branch
    /// directly to b3 on the yes edge and b1 should be tombstoned.
    #[test]
    fn collapses_trivial_jump_through_yes_edge() {
        let mut fun = ir::Func::new("f", Id(0), vec![Id(0)], Some(Type::Int));
        let b0_params = fun.intern_params(vec![Id(0)]);
        let b1_params = fun.intern_params(vec![]);
        let b2_params = fun.intern_params(vec![]);
        let b3_params = fun.intern_params(vec![]);
        let yes_br_params = fun.intern_params(vec![]);
        let no_br_params = fun.intern_params(vec![]);
        let jump_params = fun.intern_params(vec![]);
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params: b0_params,
                instructions: vec![],
                term: Some(Terminator::Branch {
                    cond: Id(0),
                    yes: (Id(1), yes_br_params),
                    no: (Id(2), no_br_params),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params: b1_params,
                instructions: vec![],
                term: Some(Terminator::Jump {
                    id: Id(3),
                    params: jump_params,
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(2),
                params: b2_params,
                instructions: vec![],
                term: Some(Terminator::Return {
                    value: Some(Id(0)),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(3),
                params: b3_params,
                instructions: vec![],
                term: Some(Terminator::Return {
                    value: Some(Id(0)),
                    span: 0,
                }),
            },
        ];

        indirect_jump(&mut fun);

        assert!(
            matches!(
                &fun.blocks[0].term,
                Some(Terminator::Branch { yes: (Id(y), _), no: (Id(n), _), .. })
                    if *y == 3 && *n == 2
            ),
            "yes edge should retarget to b3, no edge unchanged"
        );
        assert!(fun.blocks[1].tombstone, "trivial b1 must be tombstoned");
        assert!(!fun.blocks[3].tombstone, "real target b3 must survive");
    }

    /// A non-trivial intermediate (jump block with instructions) is not
    /// collapsed; its edge must point to the original block.
    #[test]
    fn leaves_non_trivial_intermediate_alone() {
        let mut fun = ir::Func::new("f", Id(0), vec![Id(0)], Some(Type::Int));
        let b0_params = fun.intern_params(vec![Id(0)]);
        let b1_params = fun.intern_params(vec![]);
        let b2_params = fun.intern_params(vec![]);
        let b3_params = fun.intern_params(vec![]);
        let yes_br_params = fun.intern_params(vec![]);
        let no_br_params = fun.intern_params(vec![]);
        let jump_params = fun.intern_params(vec![]);
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params: b0_params,
                instructions: vec![],
                term: Some(Terminator::Branch {
                    cond: Id(0),
                    yes: (Id(1), yes_br_params),
                    no: (Id(2), no_br_params),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params: b1_params,
                instructions: vec![ir::Instr::LoadConst {
                    dst: ir::TypeId {
                        id: Id(7),
                        ty: Type::Int,
                    },
                    value: ir::constant::Const::Int(0),
                    span: 0,
                }],
                term: Some(Terminator::Jump {
                    id: Id(3),
                    params: jump_params,
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(2),
                params: b2_params,
                instructions: vec![],
                term: Some(Terminator::Return {
                    value: Some(Id(0)),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(3),
                params: b3_params,
                instructions: vec![],
                term: Some(Terminator::Return {
                    value: Some(Id(0)),
                    span: 0,
                }),
            },
        ];

        indirect_jump(&mut fun);

        assert!(
            matches!(
                &fun.blocks[0].term,
                Some(Terminator::Branch { yes: (Id(y), _), .. }) if *y == 1
            ),
            "yes edge must stay on b1 (it has instructions)"
        );
        assert!(!fun.blocks[1].tombstone);
    }
}
