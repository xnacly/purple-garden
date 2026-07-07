use purple_garden_ir as ir;

/// Block-local rewrites for `Load` and `Store` instructions.
pub fn load_store_fold(fun: &mut ir::Func<'_>) {
    for block in &mut fun.blocks {
        if block.tombstone {
            continue;
        }

        fold_block(&mut block.instructions);
    }
}

fn fold_block(_instructions: &mut [ir::Instr<'_>]) {}

#[cfg(test)]
mod tests {
    use super::load_store_fold;
    use purple_garden_ir::{Block, Func, Id, Instr, Terminator, ptype::Type};

    #[test]
    fn skeleton_preserves_instructions() {
        let mut fun = Func::new("f", Id(0), Vec::new(), Some(Type::Int));
        let params = fun.intern_params(Vec::new());
        fun.blocks.push(Block {
            tombstone: false,
            id: Id(0),
            params,
            instructions: vec![Instr::Noop],
            term: Some(Terminator::Return {
                value: None,
                span: 0,
            }),
        });

        load_store_fold(&mut fun);

        assert!(matches!(
            fun.blocks[0].instructions.as_slice(),
            [Instr::Noop]
        ));
    }
}
