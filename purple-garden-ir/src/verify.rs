//! Structural invariants the backends rely on without checking them.
//!
//! Both register allocators (bytecode and JIT) treat a block as self-contained: values enter it
//! as its params, everything else it uses it defines itself. A use of a value defined in another
//! block reads whatever its register holds at that point, for a pointer that is an arbitrary
//! memory access. [`Func::verify`] checks this after lowering and after every optimisation pass
//! in debug builds, so a pass breaking it fails loudly at the pass instead of miscompiling.

use std::{alloc::Allocator, collections::HashSet};

use crate::{Func, Id, ParamsId, Terminator};

impl<A: Allocator> Func<'_, A> {
    /// The first violation of the invariants, as a message naming the function and block:
    ///
    /// - a block only uses its own params and the values it defines before the use
    /// - an edge passes exactly as many args as its target block takes params
    /// - an edge targets a live block
    ///
    /// # Errors
    ///
    /// The first violation found.
    pub fn verify(&self) -> Result<(), String> {
        for block in self.blocks.iter().filter(|b| !b.tombstone) {
            let at = |what: String| format!("{}: b{}: {what}", self.name, block.id.0);
            let mut defined: HashSet<Id> = self.params(block.params).iter().copied().collect();
            let check = |id: Id, defined: &HashSet<Id>, user: &str| {
                if defined.contains(&id) {
                    Ok(())
                } else {
                    Err(at(format!(
                        "%v{} used by {user} is neither a param nor defined before",
                        id.0
                    )))
                }
            };
            for (i, instr) in block.instructions.iter().enumerate() {
                let mut result = Ok(());
                self.for_each_use_of_instr(instr, |id| {
                    if result.is_ok() {
                        result = check(id, &defined, &format!("instruction {i}"));
                    }
                });
                result?;
                if let Some(def) = Func::def_of(instr) {
                    defined.insert(def);
                }
            }
            let Some(term) = &block.term else {
                return Err(at("no terminator".into()));
            };
            let mut result = Ok(());
            self.for_each_use_of_term(term, |id| {
                if result.is_ok() {
                    result = check(id, &defined, "the terminator");
                }
            });
            result?;
            let mut result = Ok(());
            self.for_each_edge(term, |target, args| {
                if result.is_err() {
                    return;
                }
                let Some(target_block) = self.blocks.get(target.0 as usize) else {
                    result = Err(at(format!("edge to b{} which doesn't exist", target.0)));
                    return;
                };
                if target_block.tombstone {
                    result = Err(at(format!("edge to the dead b{}", target.0)));
                    return;
                }
                let (passed, taken) = (
                    self.params(args).len(),
                    self.params(target_block.params).len(),
                );
                if passed != taken {
                    result = Err(at(format!(
                        "edge to b{} passes {passed} args, it takes {taken} params",
                        target.0
                    )));
                }
            });
            result?;
        }
        Ok(())
    }

    /// Calls `f` with the target block and the args of every edge out of `term`.
    pub fn for_each_edge(&self, term: &Terminator, mut f: impl FnMut(Id, ParamsId)) {
        match term {
            Terminator::Return { .. } | Terminator::Tail { .. } => {}
            Terminator::Jump { id, params, .. } => f(*id, *params),
            Terminator::Branch { yes, no, .. }
            | Terminator::BranchCmpImm { yes, no, .. }
            | Terminator::BranchCmp { yes, no, .. } => {
                f(yes.0, yes.1);
                f(no.0, no.1);
            }
            Terminator::Switch { cases, default, .. } => {
                for case in self.cases(*cases) {
                    f(case.target.0, case.target.1);
                }
                f(default.0, default.1);
            }
        }
    }
}
