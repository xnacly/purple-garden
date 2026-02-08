mod anomaly;
mod value;

pub const REGISTER_COUNT: usize = 64;
pub use crate::vm::anomaly::Anomaly;
pub use crate::vm::value::Value;
use crate::{Args, vm::op::Op};
/// purple garden bytecode virtual machine operations
pub mod op;

#[derive(Default, Debug)]
pub struct CallFrame {
    pub return_to: usize,
}

pub type BuiltinFn<'vm> = fn(&mut Vm<'vm>, &[Value<'vm>]) -> Option<Value<'vm>>;

#[repr(C)]
#[derive(Debug)]
pub struct Vm<'vm> {
    pub r: [Value<'vm>; REGISTER_COUNT],
    pub pc: usize,

    pub frames: Vec<CallFrame>,

    pub bytecode: Vec<Op>,
    pub globals: Vec<Value<'vm>>,

    /// backtrace holds a list of indexes into the bytecode, pointing to the definition site of the
    /// function the virtual machine currently executes in, this behaviour only occurs if
    /// --backtrace was passed as an option to the interpreter
    pub backtrace: Vec<usize>,

    config: &'vm Args,
}

/// trap in the vm; return Err(<anomaly>) if expr == true
#[allow(unused)]
#[cfg(feature = "nightly")]
macro_rules! trap_if {
    ($condition:expr, $anomaly:expr) => {
        if std::hint::unlikely($condition) {
            return Err($anomaly);
        }
    };
}

/// non-nightly fallback for trap_if
#[allow(unused)]
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
    pub fn new(config: &'vm Args) -> Self {
        Self {
            r: [const { Value::UnDef }; REGISTER_COUNT],
            frames: Vec::with_capacity(64),
            pc: 0,
            bytecode: Vec::new(),
            globals: Vec::new(),
            backtrace: Vec::new(),
            config,
        }
    }

    pub fn run(&mut self) -> Result<(), Anomaly> {
        while self.pc < self.bytecode.len() {
            let instruction = unsafe_get!(self.bytecode, self.pc);

            crate::trace!("[vm][{:04}] {:?}", self.pc, instruction);

            match instruction {
                Op::Nop => {}
                Op::LoadI { dst, value } => {
                    *unsafe_get_mut!(self.r, *dst) = Value::Int(*value as i64)
                }
                Op::LoadG { dst, idx } => {
                    *unsafe_get_mut!(self.r, *dst) = unsafe_get!(self.globals, *idx).clone();
                }
                Op::IAdd { dst, lhs, rhs } => {
                    let lhs = unsafe_get!(self.r, *lhs);
                    let rhs = unsafe_get!(self.r, *rhs);

                    let result = match (lhs, rhs) {
                        (Value::Int(l), Value::Int(r)) => Value::Int(l + r),
                        _ => unimplemented!(),
                    };

                    *unsafe_get_mut!(self.r, *dst) = result;
                }
                Op::ISub { dst, lhs, rhs } => {
                    let lhs = unsafe_get!(self.r, *lhs);
                    let rhs = unsafe_get!(self.r, *rhs);

                    let result = match (lhs, rhs) {
                        (Value::Int(l), Value::Int(r)) => Value::Int(l - r),
                        _ => unimplemented!(),
                    };

                    *unsafe_get_mut!(self.r, *dst) = result;
                }
                Op::IMul { dst, lhs, rhs } => {
                    let lhs = unsafe_get!(self.r, *lhs);
                    let rhs = unsafe_get!(self.r, *rhs);

                    let result = match (lhs, rhs) {
                        (Value::Int(l), Value::Int(r)) => Value::Int(l * r),
                        _ => unimplemented!(),
                    };

                    *unsafe_get_mut!(self.r, *dst) = result;
                }
                Op::IDiv { dst, lhs, rhs } => {
                    let lhs = unsafe_get!(self.r, *lhs);
                    let rhs = unsafe_get!(self.r, *rhs);

                    let result = match (lhs, rhs) {
                        (Value::Int(l), Value::Int(r)) => Value::Int(l / r),
                        (Value::Int(_), Value::Int(0)) | (Value::Double(_), Value::Int(0)) => {
                            return Err(Anomaly::DivisionByZero { pc: self.pc });
                        }
                        _ => unimplemented!(),
                    };

                    *unsafe_get_mut!(self.r, *dst) = result;
                }
                // TODO: eq should only work for i, the comparison for D and Str should be compiled
                // to something else
                Op::Eq { dst, lhs, rhs } => {
                    let lhs = unsafe_get!(self.r, *lhs);
                    let rhs = unsafe_get!(self.r, *rhs);

                    *unsafe_get_mut!(self.r, *dst) = match (lhs, rhs) {
                        (Value::True, Value::True) | (Value::False, Value::False) => true,
                        (Value::Double(lhs), Value::Double(rhs)) => (lhs - rhs) < f64::EPSILON,
                        (Value::Int(lhs), Value::Int(rhs)) => lhs == rhs,
                        (Value::Str(lhs), Value::Str(rhs)) => lhs == rhs,
                        (Value::String(lhs), Value::Str(rhs)) => lhs == rhs,
                        _ => false,
                    }
                    .into()
                }
                Op::BNot { dst, src } => {
                    *unsafe_get_mut!(self.r, *dst) = match unsafe_get!(self.r, *src) {
                        Value::True => Value::False,
                        Value::False => Value::True,
                        _ => return Err(Anomaly::Unimplemented { pc: self.pc }),
                    }
                }
                Op::Mov { dst, src } => {
                    *unsafe_get_mut!(self.r, *dst) = unsafe_get!(self.r, *src).clone();
                }
                Op::Jmp { target } => {
                    self.pc = *target as usize;
                    continue;
                }
                Op::JmpF { target, cond } => {
                    if let Value::True = unsafe_get!(self.r, *cond) {
                        self.pc = *target as usize;
                        continue;
                    }
                }
                Op::Call { func } => {
                    if self.config.backtrace {
                        self.backtrace.push(*func as usize);
                    }

                    self.frames.push(CallFrame { return_to: self.pc });
                    self.pc = *func as usize;
                    continue;
                }
                Op::Ret => {
                    if self.config.backtrace {
                        self.backtrace.pop();
                    }
                    let Some(frame) = self.frames.pop() else {
                        unreachable!("Op::Ret had no frame to drop, this is a compiler bug");
                    };
                    self.pc = frame.return_to;
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

#[cfg(test)]
mod ops {}
