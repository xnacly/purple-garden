use std::ops::Range;

/// [LinearScanRegalloc] is a constraint solver over an purple garden IR [crate::ir::Func] according to the [Constraint] it is instanciated with.
pub struct LinearScanRegalloc {
    constraint: Constraint,
}

pub struct Constraint {
    fixed_inputs: Range<u8>,
    fixed_outputs: Range<u8>,
    clobbers: Range<u8>,
}

impl Constraint {
    const PURPLE_GARDEN_VIRTUAL_MACHINE: Self = Self {
        fixed_inputs: todo!(),
        fixed_outputs: todo!(),
        clobbers: todo!(),
    };
    const AMD64: Self = Self {
        fixed_inputs: todo!(),
        fixed_outputs: todo!(),
        clobbers: todo!(),
    };
    const AARCH64: Self = Self {
        fixed_inputs: todo!(),
        fixed_outputs: todo!(),
        clobbers: todo!(),
    };
}

impl LinearScanRegalloc {
    pub fn with_constraint(constraint: Constraint) -> Self {
        Self { constraint }
    }

    pub fn update_livelyness_set(set: ()) {}
}
