use crate::ir;

/// merges a br to block A indirected by a single jmp in block B into a direct br to A:
///
///     b1:
///             %v4:Bool = True
///             br %v4, b2, b3
///     b2:
///             jmp b4(%v1) // <- redundant
///     b3:
///     b4:
///
/// Should be:
///
///     b1:
///             %v4:Bool = True
///             br %v4, b4, b3
///     b3:
///     b4:
pub fn indirect_jump(fun: &mut ir::Func) {
    for i in 0..fun.blocks.len() {
        let Some(ir::Terminator::Branch {
            cond,
            yes: (ir::Id(yes), yes_params),
            no: (ir::Id(no), no_params),
            ..
        }) = fun.blocks[i].term.clone()
        else {
            continue;
        };

        let yes_target = &mut fun.blocks[yes as usize];
        let yes_edge = if yes_target.instructions.is_empty() {
            if let Some(ir::Terminator::Jump { id, params }) = &yes_target.term {
                yes_target.tombstone = true;
                Some((*id, params.clone()))
            } else {
                None
            }
        } else {
            None
        };

        let no_target = &mut fun.blocks[no as usize];
        let no_edge = if no_target.instructions.is_empty() {
            if let Some(ir::Terminator::Jump { id, params }) = &no_target.term {
                no_target.tombstone = true;
                Some((*id, params.clone()))
            } else {
                None
            }
        } else {
            None
        };

        #[cfg(feature = "trace")]
        if yes_edge.is_some() || no_edge.is_some() {
            opt_trace!(
                "ir::indirect_jump",
                "replaced a noop block with a direct jump"
            );
        }

        fun.blocks[i].term = Some(ir::Terminator::Branch {
            cond,
            yes: yes_edge.unwrap_or((ir::Id(yes), yes_params)),
            no: no_edge.unwrap_or((ir::Id(no), no_params)),
        });
    }
}
