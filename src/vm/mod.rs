mod anomaly;
mod value;

pub const REGISTER_COUNT: usize = 64;
use std::hint::unreachable_unchecked;

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
        let regs = unsafe { self.r.as_mut_ptr() };
        while self.pc < self.bytecode.len() {
            let instruction = unsafe { *self.bytecode.as_mut_ptr().add(self.pc) };

            crate::trace!("[vm][{:04}] {:?}", self.pc, instruction);

            match instruction {
                Op::Nop => {}
                Op::LoadI { dst, value } => unsafe {
                    *regs.add(dst as usize) = Value::Int(value as i64);
                },
                Op::LoadG { dst, idx } => unsafe {
                    *regs.add(dst as usize) =
                        (*self.globals.as_mut_ptr().add(idx as usize)).clone();
                },
                Op::IAdd { dst, lhs, rhs } => unsafe {
                    let l = (*regs.add(lhs as usize)).as_int();
                    let r = (*regs.add(rhs as usize)).as_int();
                    *regs.add(dst as usize) = Value::Int(l + r);
                },
                Op::ISub { dst, lhs, rhs } => unsafe {
                    let l = (*regs.add(lhs as usize)).as_int();
                    let r = (*regs.add(rhs as usize)).as_int();
                    *regs.add(dst as usize) = Value::Int(l - r);
                },
                Op::IMul { dst, lhs, rhs } => unsafe {
                    let l = (*regs.add(lhs as usize)).as_int();
                    let r = (*regs.add(rhs as usize)).as_int();
                    *regs.add(dst as usize) = Value::Int(l * r);
                },
                Op::IDiv { dst, lhs, rhs } => unsafe {
                    let l = (*regs.add(lhs as usize)).as_int();
                    let r = (*regs.add(rhs as usize)).as_int();
                    trap_if!(r == 0, Anomaly::DivisionByZero { pc: self.pc });
                    *regs.add(dst as usize) = Value::Int(l / r);
                },
                // TODO: eq should only work for i, the comparison for D and Str should be compiled
                // to something else
                Op::Eq { dst, lhs, rhs } => unsafe {
                    let l = &(*regs.add(lhs as usize));
                    let r = &(*regs.add(rhs as usize));

                    *regs.add(dst as usize) = match (l, r) {
                        (Value::True, Value::True) | (Value::False, Value::False) => true,
                        (Value::Double(l), Value::Double(r)) => (l - r) < f64::EPSILON,
                        (Value::Int(l), Value::Int(r)) => l == r,
                        (Value::Str(l), Value::Str(r)) => l == r,
                        (Value::String(l), Value::Str(r)) => l == r,
                        _ => false,
                    }
                    .into()
                },
                Op::BNot { dst, src } => unsafe {
                    *regs.add(dst as usize) = Value::from(!(*regs.add(src as usize)).as_bool())
                },
                Op::Mov { dst, src } => unsafe {
                    // TODO: decide if mov dst,src should be a real mov or a copy
                    //
                    // *regs.add(dst as usize) = (*regs.add(src as usize)).clone();

                    let r_src = &mut (*regs.add(src as usize));
                    *regs.add(dst as usize) = std::mem::take(r_src);
                },
                Op::Jmp { target } => {
                    self.pc = target as usize;
                    continue;
                }
                Op::JmpF { target, cond } => unsafe {
                    if (*regs.add(cond as usize)).as_bool() {
                        self.pc = target as usize;
                        continue;
                    }
                },
                Op::Call { func } => {
                    if self.config.backtrace {
                        self.backtrace.push(func as usize);
                    }

                    self.frames.push(CallFrame { return_to: self.pc });
                    self.pc = func as usize;
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
