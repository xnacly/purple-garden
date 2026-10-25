use std::alloc::Allocator;

use crate::{Func, Id, Terminator};

impl<A: Allocator> Func<'_, A> {
    /// Per-SSA live interval, indexed by id. `(u32::MAX, 0)` marks a slot with no def; only happens
    /// for params of tombstoned blocks since SSA ids are otherwise dense.
    ///
    /// Writes into `out`, clearing first. Lets the caller reuse a buffer across function compiles
    /// so we don't allocate fresh per `cc()`.
    pub fn live_set_into<V: Allocator>(&self, out: &mut Vec<(u32, u32), V>) {
        const UNSET: (u32, u32) = (u32::MAX, 0);

        fn ensure<A: Allocator>(v: &mut Vec<(u32, u32), A>, id: u32) {
            let idx = id as usize;
            if idx >= v.len() {
                v.resize(idx + 1, UNSET);
            }
        }

        fn define<A: Allocator>(intervals: &mut Vec<(u32, u32), A>, id: Id, pos: u32) {
            ensure(intervals, id.0);
            let e = &mut intervals[id.0 as usize];
            if e.0 == u32::MAX {
                *e = (pos, pos);
            } else {
                e.0 = e.0.min(pos);
                e.1 = e.1.max(pos);
            }
        }

        fn use_value<A: Allocator>(intervals: &mut Vec<(u32, u32), A>, id: Id, pos: u32) {
            ensure(intervals, id.0);
            let e = &mut intervals[id.0 as usize];
            if e.0 == u32::MAX {
                *e = (pos, pos);
            } else {
                e.1 = e.1.max(pos);
            }
        }

        out.clear();

        // Each event takes two slots: early (pos) and late (pos+1). Instr
        // sources land on early, dst on late, so a source dying at pos P
        // and a dst born at pos P can share a register. Branch yes-shuffle
        // is early, cond and no-shuffle are late, so yes_movs (which run
        // before JmpT) can't clobber cond. bc::Cc::cc steps pos by 2 in
        // lockstep so its call-site spill checks see the same units.
        let intervals = &mut *out;
        let mut pos = 0;

        for block in &self.blocks {
            if block.tombstone {
                continue;
            }

            // A header redeclaring an already live param writes nothing, the
            // edge move into it happened in the predecessor, so it must not
            // extend the interval past the param's last real use.
            for param in self.params(block.params) {
                ensure(intervals, param.0);
                let e = &mut intervals[param.0 as usize];
                if e.0 == u32::MAX {
                    *e = (pos, pos);
                } else {
                    e.0 = e.0.min(pos);
                }
            }
            pos += 2;

            for instr in &block.instructions {
                self.for_each_use_of_instr(instr, |use_id| {
                    use_value(intervals, use_id, pos);
                });

                if let Some(def_id) = Func::def_of(instr) {
                    define(intervals, def_id, pos + 1);
                }
                pos += 2;
            }

            if let Some(term) = &block.term {
                match term {
                    Terminator::Branch {
                        cond,
                        yes: (yes_id, yes_params),
                        no: (no_id, no_params),
                        ..
                    } => {
                        // yes-shuffle runs before JmpT.
                        for &p in self.params(*yes_params) {
                            use_value(intervals, p, pos);
                        }
                        for &p in self.params(self.blocks[yes_id.0 as usize].params) {
                            define(intervals, p, pos);
                        }
                        // JmpT reads cond after yes_movs; the phase gap
                        // keeps cond's reg out of yes_dst's reach.
                        use_value(intervals, *cond, pos + 1);
                        // no-shuffle runs after JmpT.
                        for &p in self.params(*no_params) {
                            use_value(intervals, p, pos + 1);
                        }
                        for &p in self.params(self.blocks[no_id.0 as usize].params) {
                            define(intervals, p, pos + 1);
                        }
                    }
                    Terminator::BranchCmpImm {
                        lhs,
                        yes: (yes_id, yes_params),
                        no: (no_id, no_params),
                        ..
                    }
                    | Terminator::BranchCmp {
                        lhs,
                        yes: (yes_id, yes_params),
                        no: (no_id, no_params),
                        ..
                    } => {
                        // Same edge-move phasing as Branch: yes shuffle first,
                        // comparison operands after yes moves, no shuffle last.
                        for &p in self.params(*yes_params) {
                            use_value(intervals, p, pos);
                        }
                        for &p in self.params(self.blocks[yes_id.0 as usize].params) {
                            define(intervals, p, pos);
                        }
                        use_value(intervals, *lhs, pos + 1);
                        if let Terminator::BranchCmp { rhs, .. } = term {
                            use_value(intervals, *rhs, pos + 1);
                        }
                        for &p in self.params(*no_params) {
                            use_value(intervals, p, pos + 1);
                        }
                        for &p in self.params(self.blocks[no_id.0 as usize].params) {
                            define(intervals, p, pos + 1);
                        }
                    }
                    // Jump/Tail shuffle dsts are NOT recorded: doing so
                    // would extend a join-block param back through every
                    // predecessor, making the call-site spill check
                    // spill (and pop-overwrite) the very value the call
                    // is computing. The IR mostly threads matching SSA
                    // ids so jump shuffles elide; the residual hazard is
                    // a known TODO (parallel-move resolver).
                    _ => {
                        self.for_each_use_of_term(term, |use_id| {
                            use_value(intervals, use_id, pos);
                        });
                    }
                }
            }
            pos += 2;
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

        // Each logical event consumes two pos slots (early + late). Within
        // an instruction, src use lands on early, dst def on late. Within
        // a Branch term, yes-shuffle is early, cond + no-shuffle are late.
        assert_eq!(live_set[0], (0, 6)); // %v0: b0 param, used by IAdd and yes-shuffle
        assert_eq!(live_set[1], (3, 7)); // %v1: defined by LoadConst (late), used by IAdd, no-shuffle
        assert_eq!(live_set[2], (5, 7)); // %v2: defined by IAdd (late), used by Branch cond (late)
        assert_eq!(live_set[3], (6, 10)); // %v3: yes-shuffle dst + b1 param, used by Return
        assert_eq!(live_set[4], (7, 14)); // %v4: no-shuffle dst + b2 param, used by Cast
        assert_eq!(live_set[5], (15, 16)); // %v5: Cast dst (late), used by Return
    }
}
