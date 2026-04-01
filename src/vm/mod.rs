pub mod anomaly;
/// purple garden bytecode virtual machine operations
pub mod op;
pub mod value;

pub const REGISTER_COUNT: usize = 64;

use crate::config::Config;
pub use crate::vm::anomaly::Anomaly;
pub use crate::vm::value::Value;
use op::Op;

pub type BuiltinFn = fn(&mut Vm) -> Value;
pub fn syscall_unimplemented<'vm>(vm: &mut Vm<'vm>) -> Result<Value, Anomaly> {
    Err(Anomaly::InvalidSyscall { pc: vm.pc })
}

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
    pub strings: Vec<String>,

    /// backtrace holds a list of indexes into the bytecode, pointing to the definition site of the
    /// function the virtual machine currently executes in, this behaviour only occurs if
    /// --backtrace was passed as an option to the interpreter
    pub backtrace: Vec<usize>,

    // TODO: replace this with an array
    pub syscalls: Vec<BuiltinFn>,

    config: &'vm Config,
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
    pub fn new(config: &'vm Config) -> Self {
        Self {
            r: [const { Value(0) }; REGISTER_COUNT],
            frames: Vec::with_capacity(64),
            pc: 0,
            bytecode: Vec::new(),
            globals: Vec::new(),
            strings: Vec::new(),
            backtrace: Vec::new(),
            spilled: Vec::with_capacity(REGISTER_COUNT),
            syscalls: Vec::new(),
            config,
        }
    }

    /// creates a new string in [vm::heap_strings], a reference to it into [vm::strings] and
    /// returns the index into the latter
    pub fn new_string(&mut self, s: String) -> usize {
        let idx = self.strings.len();
        self.strings.push(s);
        idx
    }

    pub fn run(&mut self) -> Result<(), Anomaly> {
        let regs = self.r.as_mut_ptr();
        let instructions = self.bytecode.as_mut_ptr();
        let instructions_len = self.bytecode.len();
        let globals = self.globals.as_mut_ptr();
        let syscalls = self.syscalls.as_mut_ptr();

        macro_rules! r {
            ($n:tt) => {
                (&*regs.add($n as usize))
            };
        }

        macro_rules! r_mut {
            ($n:tt) => {
                *regs.add($n as usize)
            };
        }

        let mut pc = self.pc;

        while pc < instructions_len {
            let op = unsafe { *instructions.add(pc) };
            crate::trace!("[vm][{:04}] {:?}", pc, op);

            match op {
                Op::Nop => {}
                Op::LoadI { dst, value } => unsafe {
                    r_mut!(dst) = Value::from(value as i64);
                },
                Op::LoadG { dst, idx } => unsafe { r_mut!(dst) = *globals.add(idx as usize) },
                Op::IAdd { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_int();
                    let r = r!(rhs).as_int();
                    r_mut!(dst) = Value::from(l + r);
                },
                Op::ISub { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_int();
                    let r = r!(rhs).as_int();
                    r_mut!(dst) = Value::from(l - r);
                },
                Op::IMul { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_int();
                    let r = r!(rhs).as_int();
                    r_mut!(dst) = Value::from(l * r);
                },
                Op::IDiv { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_int();
                    let r = r!(rhs).as_int();
                    trap_if!(r == 0, Anomaly::DivisionByZero { pc: pc });
                    r_mut!(dst) = Value::from(l / r);
                },
                Op::IEq { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_int();
                    let r = r!(rhs).as_int();
                    r_mut!(dst) = Value::from(l == r)
                },
                Op::IGt { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_int();
                    let r = r!(rhs).as_int();
                    r_mut!(dst) = Value::from(l > r)
                },
                Op::ILt { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_int();
                    let r = r!(rhs).as_int();
                    r_mut!(dst) = Value::from(l < r)
                },
                Op::DAdd { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_f64();
                    let r = r!(rhs).as_f64();
                    r_mut!(dst) = Value::from(l + r);
                },
                Op::DSub { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_f64();
                    let r = r!(rhs).as_f64();
                    r_mut!(dst) = Value::from(l - r);
                },
                Op::DMul { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_f64();
                    let r = r!(rhs).as_f64();
                    r_mut!(dst) = Value::from(l * r);
                },
                Op::DDiv { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_f64();
                    let r = r!(rhs).as_f64();
                    trap_if!(r == 0 as f64, Anomaly::DivisionByZero { pc: pc });
                    r_mut!(dst) = Value::from(l / r);
                },
                Op::DGt { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_f64();
                    let r = r!(rhs).as_f64();
                    r_mut!(dst) = Value::from(l > r);
                },
                Op::DLt { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_f64();
                    let r = r!(rhs).as_f64();
                    r_mut!(dst) = Value::from(l < r);
                },
                Op::BEq { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_bool();
                    let r = r!(rhs).as_bool();
                    r_mut!(dst) = Value::from(l == r);
                },
                Op::Mov { dst, src } => unsafe {
                    r_mut!(dst) = *r!(src);
                },
                Op::Jmp { target } => {
                    pc = target as usize;
                    continue;
                }
                Op::Tail { func } => {
                    pc = func as usize;
                    continue;
                }
                Op::JmpF { target, cond } => unsafe {
                    if (r!(cond).as_bool()) {
                        pc = target as usize;
                        continue;
                    }
                },
                Op::Call { func } => {
                    if self.config.backtrace {
                        self.backtrace.push(func as usize);
                    }

                    self.frames.push(CallFrame { return_to: pc });
                    pc = func as usize;
                    continue;
                }
                Op::Sys { idx } => unsafe {
                    r_mut!(0) = (*syscalls.add(idx as usize))(self);
                },
                Op::Ret => {
                    if self.config.backtrace {
                        self.backtrace.pop();
                    }
                    let Some(frame) = self.frames.pop() else {
                        unreachable!("Op::Ret had no frame to drop, this is a compiler bug");
                    };
                    pc = frame.return_to;
                }
                Op::Push { src } => unsafe {
                    self.spilled.push(*r!(src));
                },
                Op::Pop { dst } => unsafe {
                    r_mut!(dst) = self.spilled.pop().unwrap();
                },
                Op::CastToDouble { dst, src } => unsafe {
                    r_mut!(dst) = r!(src).int_to_f64();
                },
                Op::CastToInt { dst, src } => unsafe {
                    r_mut!(dst) = r!(src).f64_to_int();
                },
                Op::CastToBool { dst, src } => unsafe {
                    r_mut!(dst) = r!(src).int_to_bool();
                },
                i => {
                    dbg!(i);
                    return Err(Anomaly::Unimplemented { pc });
                }
            }

            pc += 1;
        }

        self.pc = pc;

        Ok(())
    }
}

#[cfg(test)]
mod ops {}
