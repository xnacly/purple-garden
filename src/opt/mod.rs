use crate::op::Op;

macro_rules! opt_trace {
    ($optimisation:literal, $text:expr) => {
        #[cfg(feature = "trace")]
        println!("[opt::{}]: {}", $optimisation, $text);
    };
}

/// ir based optimisations
mod ir;

/// bytecode based optimisations, mainly peephole
mod bc;

pub fn ir(ir: ()) {
    ()
}

const WINDOW_SIZE: usize = 3;

/// Peephole optimisations
///
/// See:
/// - [Peephole optimization - wikipedia](https://en.wikipedia.org/wiki/Peephole_optimization)
/// - [W. M. McKeeman "Peephole Optimization"](https://dl.acm.org/doi/epdf/10.1145/364995.365000)
pub fn bc(bc: &mut Vec<Op>) {
    for i in 0..=bc.len().saturating_sub(WINDOW_SIZE) {
        let window = &mut bc[i..i + WINDOW_SIZE];
        // Disabled, due to https://news.ycombinator.com/item?id=46624396, specifically:
        // "[...] [This] breaks using temporary registers after the optimisation."
        //
        // bc::const_binary(window);
        bc::self_move(window);
    }

    bc.retain(|op| !matches!(op, Op::Nop))
}
