mod value;

pub const REGISTER_COUNT: usize = 32;

pub use crate::vm::value::Value;
use crate::{err::PgError, op::Op};

#[derive(Default, Debug)]
pub struct CallFrame {
    return_to: usize,
    locals_base: usize,
}

pub type BuiltinFn<'vm> = fn(&mut Vm<'vm>, &[Value<'vm>]) -> Option<Value<'vm>>;

#[derive(Debug)]
pub struct Vm<'vm> {
    pub registers: [Value<'vm>; REGISTER_COUNT],
    pub pc: usize,

    pub value_stack: Vec<Value<'vm>>,
    pub frames: Vec<CallFrame>,

    pub bytecode: Vec<Op<'vm>>,
    pub globals: Vec<Value<'vm>>,
    // TODO: how do I deal with anomaly (purple gardens idea of exceptions)
}

impl<'vm> Vm<'vm> {
    pub fn new() -> Self {
        Self {
            registers: [const { Value::UnDef }; REGISTER_COUNT],
            value_stack: Vec::with_capacity(1024),
            frames: Vec::with_capacity(REGISTER_COUNT),
            pc: 0,
            bytecode: vec![],
            globals: vec![],
        }
    }

    pub fn run(&mut self) -> Result<(), PgError> {
        for instruction in &self.bytecode {
            #[cfg(feature = "trace")]
            println!("[vm] {:#?}", instruction);

            match instruction {
                Op::LoadImm { dst, value } => self.registers[*dst as usize] = Value::Int(*value),
                Op::LoadGlobal { dst, idx } => {
                    self.registers[*dst as usize] = self.globals[*idx as usize].clone()
                }
                Op::LoadLocal { slot, dst } => {
                    let frame = self.frames.last().unwrap();
                    let idx = frame.locals_base + *slot as usize;
                    self.registers[*dst as usize] = self.value_stack[idx].clone();
                }
                Op::StoreLocal { slot, src } => {
                    let frame = self.frames.last().unwrap();
                    let idx = frame.locals_base + *slot as usize;
                    self.value_stack[idx] = self.registers[*src as usize].clone();
                }
                Op::Add { dst, lhs, rhs } => {}
                _ => todo!("{:#?}", instruction),
            }

            self.pc += 1;
        }

        Ok(())
    }
}
