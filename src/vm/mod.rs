mod anomaly;
mod value;

pub const REGISTER_COUNT: usize = 64;
pub use crate::vm::anomaly::Anomaly;
pub use crate::vm::value::Value;

use crate::op::Op;

#[derive(Default, Debug)]
pub struct CallFrame {
    pub return_to: usize,
}

pub type BuiltinFn<'vm> = fn(&mut Vm<'vm>, &[Value<'vm>]) -> Option<Value<'vm>>;

#[derive(Debug)]
pub struct Vm<'vm> {
    pub r: [Value<'vm>; REGISTER_COUNT],
    pub pc: usize,

    pub frames: Vec<CallFrame>,

    pub bytecode: Vec<Op>,
    pub globals: Vec<Value<'vm>>,
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
    pub fn new() -> Self {
        Self {
            r: [const { Value::UnDef }; REGISTER_COUNT],
            frames: Vec::with_capacity(64),
            pc: 0,
            bytecode: vec![],
            globals: vec![],
        }
    }

    pub fn run(&mut self) -> Result<(), Anomaly> {
        while self.pc < self.bytecode.len() {
            let instruction = unsafe_get!(self.bytecode, self.pc);

            crate::trace!("[vm][{:04}] {:?}", self.pc, instruction);

            match instruction {
                Op::LoadImm { dst, value } => {
                    *unsafe_get_mut!(self.r, *dst) = Value::Int(*value as i64)
                }
                Op::LoadGlobal { dst, idx } => {
                    *unsafe_get_mut!(self.r, *dst) = unsafe_get!(self.globals, *idx).clone();
                }
                Op::Add { dst, lhs, rhs } => {
                    let lhs = unsafe_get!(self.r, *lhs);
                    let rhs = unsafe_get!(self.r, *rhs);

                    let result = match (lhs, rhs) {
                        (Value::Int(l), Value::Int(r)) => Value::Int(l + r),
                        (Value::Double(l), Value::Int(r)) => Value::Double(l + *r as f64),
                        (Value::Int(l), Value::Double(r)) => Value::Double(*l as f64 + r),
                        (Value::Double(l), Value::Double(r)) => Value::Double(l + r),
                        _ => todo!(),
                    };

                    *unsafe_get_mut!(self.r, *dst) = result;
                }
                Op::Sub { dst, lhs, rhs } => {
                    let lhs = unsafe_get!(self.r, *lhs);
                    let rhs = unsafe_get!(self.r, *rhs);

                    let result = match (lhs, rhs) {
                        (Value::Int(l), Value::Int(r)) => Value::Int(l - r),
                        (Value::Double(l), Value::Int(r)) => Value::Double(l - *r as f64),
                        (Value::Int(l), Value::Double(r)) => Value::Double(*l as f64 - r),
                        (Value::Double(l), Value::Double(r)) => Value::Double(l - r),
                        _ => todo!(),
                    };

                    *unsafe_get_mut!(self.r, *dst) = result;
                }
                Op::Mul { dst, lhs, rhs } => {
                    let lhs = unsafe_get!(self.r, *lhs);
                    let rhs = unsafe_get!(self.r, *rhs);

                    let result = match (lhs, rhs) {
                        (Value::Int(l), Value::Int(r)) => Value::Int(l * r),
                        (Value::Double(l), Value::Int(r)) => Value::Double(l * *r as f64),
                        (Value::Int(l), Value::Double(r)) => Value::Double(*l as f64 * r),
                        (Value::Double(l), Value::Double(r)) => Value::Double(l * r),
                        _ => todo!(),
                    };

                    *unsafe_get_mut!(self.r, *dst) = result;
                }
                Op::Div { dst, lhs, rhs } => {
                    let lhs = unsafe_get!(self.r, *lhs);
                    let rhs = unsafe_get!(self.r, *rhs);

                    let result = match (lhs, rhs) {
                        (Value::Int(_), Value::Int(0)) | (Value::Double(_), Value::Int(0)) => {
                            return Err(Anomaly::DivisionByZero { pc: self.pc });
                        }
                        (Value::Int(l), Value::Int(r)) => Value::Int(l / r),
                        // promoting to Double necessary
                        (Value::Double(l), Value::Int(r)) => Value::Double(l / (*r as f64)),
                        (Value::Int(l), Value::Double(r)) => Value::Double((*l as f64) / r),
                        _ => todo!(),
                    };

                    *unsafe_get_mut!(self.r, *dst) = result;
                }
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
                Op::Not { dst, src } => {
                    *unsafe_get_mut!(self.r, *dst) = match unsafe_get!(self.r, *src) {
                        Value::True => Value::False,
                        Value::False => Value::True,
                        Value::Int(inner) => Value::Int(inner * -1),
                        Value::Double(inner) => Value::Double(inner * -1.0),
                        _ => todo!(),
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
                    self.frames.push(CallFrame { return_to: self.pc });
                    self.pc = *func as usize;
                    continue;
                }
                Op::Ret => {
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
mod ops {
    use crate::{
        op::Op,
        vm::{CallFrame, Value, Vm},
    };

    #[test]
    fn load_global() {
        let mut vm = Vm::new();
        vm.globals = vec![Value::Double(3.1415)];
        vm.bytecode = vec![Op::LoadGlobal { dst: 0, idx: 0 }];
        if let Err(err) = vm.run() {
            panic!("{} failed due to {:?}", "test", err);
        }

        assert_eq!(vm.r[0], Value::Double(3.1415))
    }

    macro_rules! cases {
        ($($ident:tt :: $bytecode:expr => $expected:expr),*) => {
            $(
                #[test]
                fn $ident() {
                    let mut vm = Vm::new();
                    vm.bytecode = $bytecode;
                    vm.frames.push(CallFrame {
                        return_to: 0,
                    });
                    if let Err(err) = vm.run() {
                        panic!("{} failed due to {:?}", stringify!($ident), err);
                    }

                    assert_eq!(vm.r[0], $expected.into())
                }
            )*
        };
    }

    cases!(
        load_imm :: vec![Op::LoadImm{dst: 0, value: 0xDEAD}] => 0xDEAD,
        add :: vec![
            Op::LoadImm{dst: 0, value: 5},
            Op::LoadImm{dst: 1, value: 7},
            Op::Add {dst:0, lhs: 0, rhs: 1}
        ] => 12,
        sub :: vec![
            Op::LoadImm{dst: 0, value: 5},
            Op::LoadImm{dst: 1, value: 7},
            Op::Sub {dst:0, lhs: 0, rhs: 1}
        ] => -2,
        div :: vec![
            Op::LoadImm{dst: 0, value: 15},
            Op::LoadImm{dst: 1, value: 3},
            Op::Div {dst:0, lhs: 0, rhs: 1}
        ] => 5,
        mul :: vec![
            Op::LoadImm{dst: 0, value: 15},
            Op::LoadImm{dst: 1, value: 3},
            Op::Mul {dst:0, lhs: 0, rhs: 1}
        ] => 45,
        eq :: vec![
            Op::LoadImm{dst: 0, value: 5},
            Op::LoadImm{dst: 1, value: 5},
            Op::Eq {dst:0, lhs: 0, rhs: 1}
        ] => true,
        eq_false :: vec![
            Op::LoadImm{dst: 0, value: 5},
            Op::LoadImm{dst: 1, value: 3},
            Op::Eq {dst:0, lhs: 0, rhs: 1}
        ] => false,
        not :: vec![
            Op::LoadImm{dst: 0, value: 5},
            Op::Not {dst:0, src: 0}
        ] => -5,
        not_negative :: vec![
            Op::LoadImm{dst: 0, value: -5},
            Op::Not {dst:0, src: 0}
        ] => 5,
        mov :: vec![
            Op::LoadImm{dst: 1, value: 64},
            Op::Mov {dst:0, src: 1}
        ] => 64,
        jmp :: vec![
            Op::Jmp {target: 2},
            // this is skipped:
            Op::LoadImm{dst: 0, value: 1},
            // execution resumes here
            Op::LoadImm{dst: 0, value: 2},
        ] => 2,
        jmpf :: vec![
            Op::LoadImm{dst: 0, value: 5},
            Op::LoadImm{dst: 1, value: 3},
            // this is false
            Op::Eq {dst:0, lhs: 0, rhs: 1},
            // we check for false and jump to 2 if false
            Op::JmpF {target: 2, cond: 0},
            // this is skipped:
            Op::LoadImm{dst: 0, value: 1},
            // execution resumes here
            Op::LoadImm{dst: 0, value: 2},
        ] => 2
    );
}
