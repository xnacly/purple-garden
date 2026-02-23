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
pub fn block_merge(fun: &mut ir::Func) {
    for i in 0..fun.blocks.len() {
        let Some(ir::Terminator::Branch {
            cond,
            yes: ir::Id(yes),
            no: ir::Id(no),
        }) = fun.blocks[i].term
        else {
            continue;
        };

        let yes_target = &mut fun.blocks[yes as usize];
        let mut yes_id = None;
        if yes_target.instructions.is_empty()
            && let Some(ir::Terminator::Jump { id, .. }) = yes_target.term
        {
            yes_target.tombstone = true;
            yes_id = Some(id);
        }

        let no_target = &mut fun.blocks[no as usize];
        let mut no_id = None;
        if no_target.instructions.is_empty()
            && let Some(ir::Terminator::Jump { id, .. }) = no_target.term
        {
            no_target.tombstone = true;
            no_id = Some(id);
        }

        #[cfg(feature = "trace")]
        if yes_id.is_some() || no_id.is_some() {
            opt_trace!(
                "ir::block_merge",
                "replaced a noop block with a direct jump"
            );
        }

        // TODO: once Branch has params, propagate those here from the original term
        fun.blocks[i].term = Some(ir::Terminator::Branch {
            cond,
            yes: yes_id.unwrap_or(ir::Id(yes)),
            no: no_id.unwrap_or(ir::Id(no)),
        });
    }
}
