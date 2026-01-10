mod value;

pub const REGISTER_COUNT: usize = 32;

use crate::op::Op;
pub use crate::vm::value::Value;

#[derive(Default, Debug)]
pub struct CallFrame {
    return_to: usize,
    locals_base: usize,
}

pub type BuiltinFn<'vm> = fn(&mut Vm<'vm>, &[Value]);

#[derive(Debug)]
pub struct Vm<'vm> {
    pub registers: [Option<Value<'vm>>; REGISTER_COUNT],
    pub pc: usize,

    pub value_stack: Vec<Value<'vm>>,
    pub frames: Vec<CallFrame>,

    pub bytecode: Vec<Op<'vm>>,
    pub globals: Vec<Value<'vm>>,
}

impl<'vm> Vm<'vm> {
    pub fn new() -> Self {
        Self {
            registers: [const { None }; REGISTER_COUNT],
            pc: 0,
            value_stack: Vec::with_capacity(REGISTER_COUNT),
            frames: Vec::with_capacity(REGISTER_COUNT),
            bytecode: vec![],
            globals: vec![],
        }
    }
}
