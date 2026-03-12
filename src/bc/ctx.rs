use std::collections::HashMap;

use crate::{
    bc::{Const, intern::Interner},
    ir::Id,
};

#[derive(Debug, Clone)]
pub struct Func<'fun> {
    pub name: &'fun str,
    pub pc: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Context<'ctx> {
    pub globals: Interner<Const<'ctx>>,
    pub strings: Interner<&'ctx str>,
    pub functions: HashMap<Id, Func<'ctx>>,
}

impl<'ctx> Context<'ctx> {
    pub fn intern(&mut self, constant: Const<'ctx>) -> u32 {
        if let Const::Str(str) = constant {
            let str_pool_idx = self.strings.intern(str);
            self.globals.intern(Const::Int(str_pool_idx as i64))
        } else {
            self.globals.intern(constant)
        }
    }
}
