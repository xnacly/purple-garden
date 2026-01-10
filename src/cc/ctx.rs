use std::collections::HashMap;

use crate::cc::Const;

#[derive(Debug, Default)]
pub struct Locals<'cc> {
    slots: HashMap<&'cc str, u16>,
    next_slot: u16,
}

impl<'cc> Locals<'cc> {
    pub fn define(&mut self, name: &'cc str) -> Option<u16> {
        if self.slots.contains_key(name) {
            return None;
        }

        let slot = self.next_slot;
        self.next_slot += 1;
        self.slots.insert(name, slot);
        Some(slot)
    }

    pub fn resolve(&self, name: &str) -> Option<u16> {
        self.slots.get(name).copied()
    }
}

#[derive(Debug, Default)]
pub struct Context<'ctx> {
    pub globals: HashMap<Const<'ctx>, usize>,
    pub globals_vec: Vec<Const<'ctx>>,
    pub locals: Locals<'ctx>,
}

impl<'ctx> Context<'ctx> {
    pub fn intern(&mut self, constant: Const<'ctx>) -> u32 {
        if let Some(&idx) = self.globals.get(&constant) {
            return idx as u32;
        }

        let idx = self.globals_vec.len();
        self.globals_vec.push(constant);
        self.globals.insert(constant, idx);
        idx as u32
    }
}
