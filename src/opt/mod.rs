use crate::op::Op;

macro_rules! opt_trace {
    ($optimisation:literal, $text:literal) => {
        #[cfg(feature = "trace")]
        println!("[opt::{}]: {}", $optimisation, $text);
    };
}

/// ir based optimisations
mod ir;

/// bytecode based optimisations
mod bc;

pub fn ir(ir: ()) {
    ()
}

const WINDOW_SIZE: usize = 3;

/// bytecode optimisations are done in place
pub fn bc(bc: &mut Vec<Op>) {
    let len = bc.len();
    for i in 0..len.saturating_sub(WINDOW_SIZE) {
        let window = &mut bc[i..i + WINDOW_SIZE];
        bc::const_add(window);
        bc::self_move(window);
    }

    bc.retain(|op| !matches!(op, Op::Nop))
}
