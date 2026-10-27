use std::alloc::Allocator;

use crate::{Func, Id};

impl<A: Allocator> Func<'_, A> {
    /// Per-SSA form live interval, indexed by vreg Id. Records (lo,hi) for each.
    ///
    /// Writes into `out`, clearing first. Lets the caller reuse a buffer across function compiles
    /// so we don't allocate fresh per `cc()`.
    pub fn live_set_into<V: Allocator>(&self, out: &mut Vec<(u32, u32), V>) {
        const UNSET: (u32, u32) = (u32::MAX, 0u32);

        out.clear();
        out.resize(self.next_id() as usize, UNSET);

        /// observer Id := (lo,hi), always updates lo, only fills hi, if previously unset
        fn observe<V: Allocator>(out: &mut Vec<(u32, u32), V>, id: Id, pos: u32) {
            let idx = id.0 as usize;
            if idx >= out.len() {
                out.resize(idx + 1, UNSET);
            }
            let e = &mut out[idx];
            if e.1 == 0 {
                e.1 = pos;
            }
            e.0 = pos;
        }

        let mut base = 0;
        for block in self.blocks.iter().filter(|b| !b.tombstone) {
            let block_offset = block.instructions.len();
            let offset = base;
            if let Some(term) = &block.term {
                self.for_each_use_of_term(term, |id| {
                    // term is instruction count +1
                    observe(out, id, (offset + block_offset + 1) as u32)
                });
            }

            for (i, instr) in block.instructions.iter().enumerate().rev() {
                // +1, so it cant start at 0, since thats where the params start :)
                let pos = (base + 1 + i) as u32;
                self.for_each_use_of_instr(instr, |id| observe(out, id, pos));
                if let Some(def) = Func::def_of(instr) {
                    observe(out, def, pos)
                }
            }

            for &p in self.params(block.params) {
                observe(out, p, base as u32);
            }

            // params + instructions + terminator
            base += block_offset + 2;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{ptype::Type, *};

    fn type_id(id: u32) -> TypeId<'static> {
        TypeId {
            id: Id(id),
            ty: Type::Int,
        }
    }

    #[test]
    fn live_set_tracks_block_params_instructions_and_terminators() {
        let mut fun = Func::new(
            "live",
            Id(0),
            vec![TypeId {
                id: Id(0),
                ty: Type::Int,
            }],
            Some(Type::Int),
        );
        let b0_params = fun.intern_params(vec![Id(0)]);
        let b1_params = fun.intern_params(vec![Id(3)]);
        let b2_params = fun.intern_params(vec![Id(4)]);
        let branch_yes = fun.intern_params(vec![Id(0)]);
        let branch_no = fun.intern_params(vec![Id(1)]);
        fun.blocks = vec![
            Block {
                tombstone: false,
                id: Id(0),
                params: b0_params,
                instructions: vec![
                    Instr::LoadConst {
                        dst: type_id(1),
                        value: Const::Int(1),
                        span: 0,
                    },
                    Instr::Bin {
                        op: BinOp::IAdd,
                        dst: type_id(2),
                        lhs: Id(0),
                        rhs: Id(1),
                        span: 0,
                    },
                ],
                term: Some(Terminator::Branch {
                    cond: Id(2),
                    yes: (Id(1), branch_yes),
                    no: (Id(2), branch_no),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(1),
                params: b1_params,
                instructions: vec![],
                term: Some(Terminator::Return {
                    value: Some(Id(3)),
                    span: 0,
                }),
            },
            Block {
                tombstone: false,
                id: Id(2),
                params: b2_params,
                instructions: vec![Instr::Cast {
                    dst: type_id(5),
                    from: type_id(4),
                    span: 0,
                }],
                term: Some(Terminator::Return {
                    value: Some(Id(5)),
                    span: 0,
                }),
            },
        ];

        let mut live_set = Vec::new();
        fun.live_set_into(&mut live_set);

        // lo and hi are instruction indices: a block's params sit at its base, instruction i at
        // base + 1 + i, its terminator after the last instruction and the next block right after.
        assert_eq!(live_set[0], (0, 3)); // %v0: b0 param, used by IAdd and the yes edge
        assert_eq!(live_set[1], (1, 3)); // %v1: defined by LoadConst, used by IAdd and the no edge
        assert_eq!(live_set[2], (2, 3)); // %v2: defined by IAdd, used as the Branch cond
        assert_eq!(live_set[3], (4, 5)); // %v3: b1 param, used by Return
        assert_eq!(live_set[4], (6, 7)); // %v4: b2 param, used by Cast
        assert_eq!(live_set[5], (7, 8)); // %v5: defined by Cast, used by Return
    }
}
