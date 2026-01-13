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
    for i in 0..bc.len() {
        if let Some(window) = bc.get_mut(i..i + 3) {
            bc::self_move(window);
        }
    }

    bc.retain(|op| matches!(op, Op::Nop))
}
