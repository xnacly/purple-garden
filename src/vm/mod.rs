mod anomaly;
mod value;

pub const REGISTER_COUNT: usize = 64;
use std::hint::unreachable_unchecked;

pub use crate::vm::anomaly::Anomaly;
pub use crate::vm::value::Value;
use crate::{Args, vm::op::Op};
/// purple garden bytecode virtual machine operations
pub mod op;

pub type BuiltinFn<'vm> = fn(&mut Vm<'vm>, &[Value]) -> Option<Value>;

#[derive(Default, Debug)]
pub struct CallFrame {
    pub return_to: usize,
}

#[repr(C)]
#[derive(Debug)]
pub struct Vm<'vm> {
    pub r: [Value; REGISTER_COUNT],
    pub pc: usize,

    pub frames: Vec<CallFrame>,
    /// a stack to keep values alive across recursive function invocations
    pub spilled: Vec<Value>,

    pub bytecode: Vec<Op>,
    pub globals: Vec<Value>,
    pub strings: Vec<&'vm str>,

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
            r: [const { Value(0) }; REGISTER_COUNT],
            frames: Vec::with_capacity(64),
            pc: 0,
            bytecode: Vec::new(),
            globals: Vec::new(),
            strings: Vec::new(),
            backtrace: Vec::new(),
            spilled: Vec::with_capacity(REGISTER_COUNT),
            config,
        }
    }

    pub fn run(&mut self) -> Result<(), Anomaly> {
        let regs = unsafe { self.r.as_mut_ptr() };
        let instructions = unsafe { self.bytecode.as_mut_ptr() };
        let instructions_len = self.bytecode.len();

        loop {
            if self.pc >= instructions_len {
                break;
            }

            crate::trace!("[vm][{:04}] {:?}", self.pc, self.bytecode[self.pc]);

            match unsafe { *instructions.add(self.pc) } {
                Op::Nop => {}
                Op::LoadI { dst, value } => unsafe {
                    *regs.add(dst as usize) = (value as i64).into();
                },
                Op::LoadG { dst, idx } => unsafe {
                    *regs.add(dst as usize) = *self.globals.as_mut_ptr().add(idx as usize);
                },
                Op::IAdd { dst, lhs, rhs } => unsafe {
                    let l = (*regs.add(lhs as usize)).as_int();
                    let r = (*regs.add(rhs as usize)).as_int();
                    *regs.add(dst as usize) = (l + r).into();
                },
                Op::ISub { dst, lhs, rhs } => unsafe {
                    let l = (*regs.add(lhs as usize)).as_int();
                    let r = (*regs.add(rhs as usize)).as_int();
                    *regs.add(dst as usize) = (l - r).into();
                },
                Op::IMul { dst, lhs, rhs } => unsafe {
                    let l = (*regs.add(lhs as usize)).as_int();
                    let r = (*regs.add(rhs as usize)).as_int();
                    *regs.add(dst as usize) = (l * r).into();
                },
                Op::IDiv { dst, lhs, rhs } => unsafe {
                    let l = (*regs.add(lhs as usize)).as_int();
                    let r = (*regs.add(rhs as usize)).as_int();
                    trap_if!(r == 0, Anomaly::DivisionByZero { pc: self.pc });
                    *regs.add(dst as usize) = (l / r).into();
                },
                Op::IEq { dst, lhs, rhs } => unsafe {
                    let l = &(*regs.add(lhs as usize)).as_int();
                    let r = &(*regs.add(rhs as usize)).as_int();
                    *regs.add(dst as usize) = Value::from(l == r)
                },
                Op::IGt { dst, lhs, rhs } => unsafe {
                    let l = &(*regs.add(lhs as usize)).as_int();
                    let r = &(*regs.add(rhs as usize)).as_int();
                    *regs.add(dst as usize) = (l > r).into()
                },
                Op::ILt { dst, lhs, rhs } => unsafe {
                    let l = &(*regs.add(lhs as usize)).as_int();
                    let r = &(*regs.add(rhs as usize)).as_int();
                    *regs.add(dst as usize) = (l < r).into()
                },
                Op::DAdd { dst, lhs, rhs } => unsafe {
                    let l = (*regs.add(lhs as usize)).as_f64();
                    let r = (*regs.add(rhs as usize)).as_f64();
                    *regs.add(dst as usize) = (l + r).into();
                },
                Op::DSub { dst, lhs, rhs } => unsafe {
                    let l = (*regs.add(lhs as usize)).as_f64();
                    let r = (*regs.add(rhs as usize)).as_f64();
                    *regs.add(dst as usize) = (l - r).into();
                },
                Op::DMul { dst, lhs, rhs } => unsafe {
                    let l = (*regs.add(lhs as usize)).as_f64();
                    let r = (*regs.add(rhs as usize)).as_f64();
                    *regs.add(dst as usize) = (l * r).into();
                },
                Op::DDiv { dst, lhs, rhs } => unsafe {
                    let l = (*regs.add(lhs as usize)).as_f64();
                    let r = (*regs.add(rhs as usize)).as_f64();
                    trap_if!(r == 0 as f64, Anomaly::DivisionByZero { pc: self.pc });
                    *regs.add(dst as usize) = (l / r).into();
                },
                Op::DGt { dst, lhs, rhs } => unsafe {
                    let l = &(*regs.add(lhs as usize)).as_f64();
                    let r = &(*regs.add(rhs as usize)).as_f64();
                    *regs.add(dst as usize) = (l > r).into()
                },
                Op::DLt { dst, lhs, rhs } => unsafe {
                    let l = &(*regs.add(lhs as usize)).as_f64();
                    let r = &(*regs.add(rhs as usize)).as_f64();
                    *regs.add(dst as usize) = (l < r).into()
                },
                Op::BEq { dst, lhs, rhs } => unsafe {
                    let l = &(*regs.add(lhs as usize)).as_bool();
                    let r = &(*regs.add(rhs as usize)).as_bool();
                    *regs.add(dst as usize) = Value::from(l == r)
                },
                Op::Mov { dst, src } => unsafe {
                    *regs.add(dst as usize) = (*regs.add(src as usize));
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
                Op::Push { src } => unsafe {
                    self.spilled.push(*regs.add(src as usize));
                },
                Op::Pop { dst } => unsafe {
                    *regs.add(dst as usize) = self.spilled.pop().unwrap();
                },
                Op::CastToDouble { dst, src } => unsafe {
                    *regs.add(dst as usize) = (*regs.add(src as usize)).int_to_f64();
                },
                Op::CastToInt { dst, src } => unsafe {
                    *regs.add(dst as usize) = (*regs.add(src as usize)).f64_to_int();
                },
                Op::CastToBool { dst, src } => unsafe {
                    *regs.add(dst as usize) = (*regs.add(src as usize)).int_to_bool();
                },
                i => {
                    dbg!(i);
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
