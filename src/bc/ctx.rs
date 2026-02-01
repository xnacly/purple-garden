use std::collections::HashMap;

use crate::bc::Const;

/// Used to encode binding resolution, for instance:
///
///     let x = 5
///
/// [x] is assigned a free registers and this register is used for the dst for its [rhs]
#[derive(Debug, Default)]
pub struct Local<'cc> {
    bindings: HashMap<&'cc str, u8>,
}

impl<'cc> Local<'cc> {
    pub fn bind(&mut self, name: &'cc str, r: u8) -> Option<u8> {
        if self.bindings.contains_key(name) {
            return None;
        }
        self.bindings.insert(name, r);
        Some(r)
    }

    pub fn resolve(&self, name: &'cc str) -> Option<u8> {
        self.bindings.get(name).copied()
    }
}

#[derive(Debug, Clone)]
pub struct Func<'cc> {
    pub name: &'cc str,
    pub args: u8,
    pub size: usize,
    pub pc: usize,
}

#[derive(Debug, Default)]
pub struct Context<'ctx> {
    pub globals: HashMap<Const<'ctx>, usize>,
    pub globals_vec: Vec<Const<'ctx>>,
    pub functions: HashMap<&'ctx str, Func<'ctx>>,
    pub locals: Local<'ctx>,
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
