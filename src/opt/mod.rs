use crate::vm::op::Op;

macro_rules! opt_trace {
    ($optimisation:literal, $text:expr) => {
        crate::trace!("[opt::{}]: {}", $optimisation, $text)
    };
}

/// ir based optimisations
mod ir;

/// bytecode based optimisations, mainly peephole
mod bc;

pub fn ir(ir: ()) {
    todo!("opt::ir")
}

const WINDOW_SIZE: usize = 3;

/// Peephole optimisations
///
/// See:
/// - [Peephole optimization - wikipedia](https://en.wikipedia.org/wiki/Peephole_optimization)
/// - [W. M. McKeeman "Peephole Optimization"](https://dl.acm.org/doi/epdf/10.1145/364995.365000)
pub fn bc(bc: &mut Vec<Op>) {
    if bc.len() < WINDOW_SIZE {
        return;
    }

    for i in 0..=bc.len().saturating_sub(WINDOW_SIZE) {
        let window = &mut bc[i..i + WINDOW_SIZE];
        bc::const_binary(window);
        bc::self_move(window);
    }
}
