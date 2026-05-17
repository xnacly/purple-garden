use crate::vm::op::Op;

macro_rules! opt_trace {
    ($optimisation:literal, $text:expr) => {
        crate::trace!("[opt::{}] {}", $optimisation, $text)
    };
}

/// ir based optimisations
mod ir;

/// bytecode based optimisations, mainly peephole
mod bc;

pub fn ir(ir: &mut [crate::ir::Func]) {
    for fun in ir {
        // so all other blocks.last() are valid
        if fun.blocks.is_empty() {
            continue;
        }

        ir::indirect_jump(fun);
        ir::tailcall(fun);
    }
}

const WINDOW_SIZE: usize = 2;

/// Peephole optimisations
///
/// Performed in-place, leaving NOP behind if instructions were replaced / removed to
/// keep JMP targets stable and enabling a single pass
///
/// See:
/// - [Peephole optimization - wikipedia](https://en.wikipedia.org/wiki/Peephole_optimization)
/// - [W. M. McKeeman "Peephole Optimization"](https://dl.acm.org/doi/epdf/10.1145/364995.365000)
pub fn bc(bc: &mut [Op]) {
    if bc.is_empty() {
        return;
    }

    for i in 0..bc.len() {
        if i + 3 <= bc.len() {
            bc::pack_spills(&mut bc[i..i + 3]);
        }

        let end = (i + WINDOW_SIZE).min(bc.len());
        let window = &mut bc[i..end];
        bc::self_move(window);
        if window.len() == WINDOW_SIZE {
            bc::mov_merge(window);
            bc::jmp_next(i, window);
        }
    }
}
