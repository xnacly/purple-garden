mod anomaly;
mod value;

pub const REGISTER_COUNT: usize = 32;
pub use crate::vm::anomaly::Anomaly;
pub use crate::vm::value::Value;

use crate::op::Op;

#[derive(Default, Debug)]
pub struct CallFrame {
    pub return_to: usize,
    pub locals_base: usize,
}

pub type BuiltinFn<'vm> = fn(&mut Vm<'vm>, &[Value<'vm>]) -> Option<Value<'vm>>;

#[derive(Debug)]
pub struct Vm<'vm> {
    pub registers: [Value<'vm>; REGISTER_COUNT],
    pub pc: usize,

    pub stack: Vec<Value<'vm>>,
    pub frames: Vec<CallFrame>,

    pub bytecode: Vec<Op<'vm>>,
    pub globals: Vec<Value<'vm>>,
}

/// trap in the vm; return Err(<anomaly>) if expr == true
#[cfg(feature = "nightly")]
macro_rules! trap_if {
    ($condition:expr, $anomaly:expr) => {
        if std::hint::unlikely($condition) {
            return Err($anomaly);
        }
    };
}

// stable fallback
#[cfg(not(feature = "nightly"))]
macro_rules! trap_if {
    ($condition:expr, $anomaly:expr) => {
        if $condition {
            return Err($anomaly);
        }
    };
}

macro_rules! unsafe_get_mut {
    ($arr:expr, $idx:expr) => {{ unsafe { $arr.get_unchecked_mut($idx as usize) } }};
}

macro_rules! unsafe_get {
    ($arr:expr, $idx:expr) => {{ unsafe { $arr.get_unchecked($idx as usize) } }};
}

impl<'vm> Vm<'vm> {
    pub fn new() -> Self {
        Self {
            registers: [const { Value::UnDef }; REGISTER_COUNT],
            stack: Vec::with_capacity(1024),
            frames: Vec::with_capacity(REGISTER_COUNT),
            pc: 0,
            bytecode: vec![],
            globals: vec![],
        }
    }

    pub fn run(&mut self) -> Result<(), Anomaly> {
        while self.pc < self.bytecode.len() {
            let instruction = unsafe_get!(self.bytecode, self.pc);

            #[cfg(feature = "trace")]
            println!("[vm] {:#?}", instruction);

            match instruction {
                Op::LoadImm { dst, value } => {
                    *unsafe_get_mut!(self.registers, *dst) = Value::Int(*value)
                }
                Op::LoadGlobal { dst, idx } => {
                    *unsafe_get_mut!(self.registers, *dst) =
                        unsafe_get!(self.globals, *idx).clone();
                }
                Op::LoadLocal { slot, dst } => {
                    let frame = self.frames.last().unwrap();
                    let idx = frame.locals_base + *slot as usize;
                    *unsafe_get_mut!(self.registers, *dst) = self
                        .stack
                        .get(idx)
                        .cloned()
                        .ok_or_else(|| Anomaly::UndefinedLocal { pc: self.pc })?;
                }
                Op::StoreLocal { slot, src } => {
                    let frame = self.frames.last().unwrap();
                    let idx = frame.locals_base + *slot as usize;
                    self.stack[idx] = unsafe_get!(self.registers, *src).clone();
                }
                Op::Add { dst, lhs, rhs } => {
                    let lhs = unsafe_get!(self.registers, *lhs);
                    let rhs = unsafe_get!(self.registers, *rhs);

                    let result = match (lhs, rhs) {
                        (Value::Int(l), Value::Int(r)) => Value::Int(l + r),
                        (Value::Double(l), Value::Int(r)) => Value::Double(l + *r as f64),
                        (Value::Int(l), Value::Double(r)) => Value::Double(*l as f64 + r),
                        (Value::Double(l), Value::Double(r)) => Value::Double(l + r),
                        _ => return Err(Anomaly::TypeIncompatible { pc: self.pc }),
                    };

                    *unsafe_get_mut!(self.registers, *dst) = result;
                }
                Op::Sub { dst, lhs, rhs } => {
                    let lhs = unsafe_get!(self.registers, *lhs);
                    let rhs = unsafe_get!(self.registers, *rhs);

                    let result = match (lhs, rhs) {
                        (Value::Int(l), Value::Int(r)) => Value::Int(l - r),
                        (Value::Double(l), Value::Int(r)) => Value::Double(l - *r as f64),
                        (Value::Int(l), Value::Double(r)) => Value::Double(*l as f64 - r),
                        (Value::Double(l), Value::Double(r)) => Value::Double(l - r),
                        _ => return Err(Anomaly::TypeIncompatible { pc: self.pc }),
                    };

                    *unsafe_get_mut!(self.registers, *dst) = result;
                }
                Op::Mul { dst, lhs, rhs } => {
                    let lhs = unsafe_get!(self.registers, *lhs);
                    let rhs = unsafe_get!(self.registers, *rhs);

                    let result = match (lhs, rhs) {
                        (Value::Int(l), Value::Int(r)) => Value::Int(l * r),
                        (Value::Double(l), Value::Int(r)) => Value::Double(l * *r as f64),
                        (Value::Int(l), Value::Double(r)) => Value::Double(*l as f64 * r),
                        (Value::Double(l), Value::Double(r)) => Value::Double(l * r),
                        _ => return Err(Anomaly::TypeIncompatible { pc: self.pc }),
                    };

                    *unsafe_get_mut!(self.registers, *dst) = result;
                }
                Op::Div { dst, lhs, rhs } => {
                    let lhs = unsafe_get!(self.registers, *lhs);
                    let rhs = unsafe_get!(self.registers, *rhs);

                    let result = match (lhs, rhs) {
                        (Value::Int(_), Value::Int(0)) | (Value::Double(_), Value::Int(0)) => {
                            return Err(Anomaly::DivisionByZero { pc: self.pc });
                        }
                        (Value::Int(l), Value::Int(r)) => Value::Int(l / r),
                        // promoting to Double necessary
                        (Value::Double(l), Value::Int(r)) => Value::Double(l / (*r as f64)),
                        (Value::Int(l), Value::Double(r)) => Value::Double((*l as f64) / r),
                        (_, _) => return Err(Anomaly::TypeIncompatible { pc: self.pc }),
                    };

                    *unsafe_get_mut!(self.registers, *dst) = result;
                }
                _ => {
                    dbg!(instruction);
                    return Err(Anomaly::Unimplemented { pc: self.pc });
                }
            }

            self.pc += 1;
        }

        Ok(())
    }
}
