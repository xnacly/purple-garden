use std::collections::HashMap;

use crate::{bc::Const, ir::Id};

#[derive(Debug, Clone)]
pub struct Func<'fun> {
    pub name: &'fun str,
    pub pc: usize,
}

#[derive(Debug, Default, Clone)]
pub struct Context<'ctx> {
    pub globals: HashMap<Const<'ctx>, usize>,
    pub globals_vec: Vec<Const<'ctx>>,
    pub strings_vec: Vec<&'ctx str>,
    pub functions: HashMap<Id, Func<'ctx>>,
}

impl<'ctx> Context<'ctx> {
    pub fn intern(&mut self, constant: Const<'ctx>) -> u32 {
        if let Some(&idx) = self.globals.get(&constant) {
            return idx as u32;
        }

        let idx = self.globals_vec.len();
        if let Const::Str(str) = constant {
            let str_pool_idx = self.strings_vec.len() as i64;
            self.strings_vec.push(str);
            self.globals_vec.push(Const::Int(str_pool_idx));
        } else {
            self.globals_vec.push(constant);
        };

        self.globals.insert(constant, idx);
        idx as u32
    }
}
