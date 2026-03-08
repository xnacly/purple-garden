use crate::ir::{self, Instr};

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
        if yes_edge.is_some() {
            opt_trace!("ir::indirect_jump", format!("b{yes} is now a tombstone"));
        }

        #[cfg(feature = "trace")]
        if no_edge.is_some() {
            opt_trace!("ir::indirect_jump", format!("b{no} is now a tombstone"));
        }

        fun.blocks[i].term = Some(ir::Terminator::Branch {
            cond,
            yes: yes_edge.unwrap_or((ir::Id(yes), yes_params)),
            no: no_edge.unwrap_or((ir::Id(no), no_params)),
        });
    }
}

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
/// ```
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
    let trivial_return = match (&last.instructions[..], &last.term) {
        ([], Some(ir::Terminator::Return(_))) => true,
        _ => false,
    };

    for i in 0..fun.blocks.len() {
        let block = &mut fun.blocks[i];
        if block.tombstone {
            continue;
        }

        // last instruction must be a call
        let Some(Instr::Call { dst, func, args }) = block.instructions.last().cloned() else {
            continue;
        };

        let mut is_tail = false;

        match &block.term {
            // Pattern A: direct return
            Some(ir::Terminator::Return(Some(v))) if v.0 == dst.id.0 => {
                is_tail = true;
            }

            // Pattern B: jump to canonical return block
            Some(ir::Terminator::Jump {
                id: ir::Id(id),
                params: jump_params,
            }) if *id == last_id as u32
                && trivial_return
                && jump_params.len() == 1
                && jump_params[0].0 == dst.id.0 =>
            {
                is_tail = true;
            }

            _ => {}
        }

        if !is_tail {
            continue;
        }

        let tail = Instr::Tail { dst, func, args };

        opt_trace!(
            "ir::tailcall",
            format!(
                "replaced `{}` with `{}`",
                &block.instructions.last().unwrap(),
                &tail
            )
        );

        *block.instructions.last_mut().unwrap() = tail;
    }
}
