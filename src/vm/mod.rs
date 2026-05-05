pub mod anomaly;
/// purple garden bytecode virtual machine operations
pub mod op;
pub mod value;

pub const REGISTER_COUNT: usize = 64;

use crate::config::Config;
pub use crate::vm::anomaly::Anomaly;
pub use crate::vm::value::Value;
use op::Op;

pub type BuiltinFn = fn(&mut Vm) -> Result<Value, Anomaly>;
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
    r: [Value; REGISTER_COUNT],
    pub pc: usize,

    frames: Vec<CallFrame>,
    /// a stack to keep values alive across recursive function invocations
    spilled: Vec<Value>,

    pub bytecode: Vec<Op>,
    pub globals: Vec<Value>,
    pub strings: Vec<Box<str>>,

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
        self.strings.push(s.into_boxed_str());
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

            match op {
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
                    trap_if!(r == 0, Anomaly::DivisionByZero { pc });
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
                    trap_if!(r == 0 as f64, Anomaly::DivisionByZero { pc });
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
                    if self.config.backtrace {
                        self.backtrace.push(func as usize);
                    }
                    pc = func as usize;
                    continue;
                }
                Op::JmpF { target, cond } => unsafe {
                    if r!(cond).as_bool() {
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
                    r_mut!(0) = (*syscalls.add(idx as usize))(self)?;
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
                Op::Nop => {}
            }

            pc += 1;
        }

        self.pc = pc;

        Ok(())
    }

    #[inline(always)]
    /// access register [idx] by indexing [vm::r]
    pub fn r(&self, idx: usize) -> &Value {
        unsafe { &*self.r.as_ptr().add(idx) }
    }

    #[inline(always)]
    /// access register [idx] mutably by indexing [vm::r]
    pub fn r_mut(&mut self, idx: usize) -> &mut Value {
        unsafe { &mut *self.r.as_mut_ptr().add(idx) }
    }
}

#[cfg(test)]
mod ops {
    use super::*;
    use crate::config::Config;

    fn run(bytecode: Vec<Op>, config: &Config) -> Vm<'_> {
        let mut vm = Vm::new(config);
        vm.bytecode = bytecode;
        vm.run().expect("vm run failed");
        vm
    }

    fn run_err(bytecode: Vec<Op>, config: &Config) -> Anomaly {
        let mut vm = Vm::new(config);
        vm.bytecode = bytecode;
        vm.run().expect_err("vm run unexpectedly succeeded")
    }

    #[test]
    fn iadd() {
        let cfg = Config::default();
        let vm = run(
            vec![
                Op::LoadI { dst: 0, value: 7 },
                Op::LoadI { dst: 1, value: 35 },
                Op::IAdd {
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                },
            ],
            &cfg,
        );
        assert_eq!(vm.r(2).as_int(), 42);
    }

    #[test]
    fn isub() {
        let cfg = Config::default();
        let vm = run(
            vec![
                Op::LoadI { dst: 0, value: 50 },
                Op::LoadI { dst: 1, value: 8 },
                Op::ISub {
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                },
            ],
            &cfg,
        );
        assert_eq!(vm.r(2).as_int(), 42);
    }

    #[test]
    fn imul() {
        let cfg = Config::default();
        let vm = run(
            vec![
                Op::LoadI { dst: 0, value: 6 },
                Op::LoadI { dst: 1, value: 7 },
                Op::IMul {
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                },
            ],
            &cfg,
        );
        assert_eq!(vm.r(2).as_int(), 42);
    }

    #[test]
    fn idiv() {
        let cfg = Config::default();
        let vm = run(
            vec![
                Op::LoadI { dst: 0, value: 84 },
                Op::LoadI { dst: 1, value: 2 },
                Op::IDiv {
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                },
            ],
            &cfg,
        );
        assert_eq!(vm.r(2).as_int(), 42);
    }

    #[test]
    fn idiv_by_zero_traps() {
        let cfg = Config::default();
        let err = run_err(
            vec![
                Op::LoadI { dst: 0, value: 1 },
                Op::LoadI { dst: 1, value: 0 },
                Op::IDiv {
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                },
            ],
            &cfg,
        );
        assert!(matches!(err, Anomaly::DivisionByZero { .. }));
    }

    #[test]
    fn ddiv_by_zero_traps() {
        let cfg = Config::default();
        let mut vm = Vm::new(&cfg);
        vm.globals.push(Value::from(4.0_f64));
        vm.globals.push(Value::from(0.0_f64));
        vm.bytecode = vec![
            Op::LoadG { dst: 0, idx: 0 },
            Op::LoadG { dst: 1, idx: 1 },
            Op::DDiv {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ];
        let err = vm.run().expect_err("ddiv by zero should trap");
        assert!(matches!(err, Anomaly::DivisionByZero { .. }));
    }

    #[test]
    fn int_compare() {
        let cfg = Config::default();
        let vm = run(
            vec![
                Op::LoadI { dst: 0, value: 3 },
                Op::LoadI { dst: 1, value: 5 },
                Op::IEq {
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                },
                Op::ILt {
                    dst: 3,
                    lhs: 0,
                    rhs: 1,
                },
                Op::IGt {
                    dst: 4,
                    lhs: 0,
                    rhs: 1,
                },
            ],
            &cfg,
        );
        assert!(!vm.r(2).as_bool());
        assert!(vm.r(3).as_bool());
        assert!(!vm.r(4).as_bool());
    }

    #[test]
    fn double_arith() {
        let cfg = Config::default();
        let mut vm = Vm::new(&cfg);
        vm.globals.push(Value::from(1.5_f64));
        vm.globals.push(Value::from(2.5_f64));
        vm.bytecode = vec![
            Op::LoadG { dst: 0, idx: 0 },
            Op::LoadG { dst: 1, idx: 1 },
            Op::DAdd {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
            Op::DSub {
                dst: 3,
                lhs: 1,
                rhs: 0,
            },
            Op::DMul {
                dst: 4,
                lhs: 0,
                rhs: 1,
            },
            Op::DDiv {
                dst: 5,
                lhs: 1,
                rhs: 0,
            },
        ];
        vm.run().unwrap();
        assert_eq!(vm.r(2).as_f64(), 4.0);
        assert_eq!(vm.r(3).as_f64(), 1.0);
        assert_eq!(vm.r(4).as_f64(), 3.75);
        assert_eq!(vm.r(5).as_f64(), 2.5 / 1.5);
    }

    #[test]
    fn mov() {
        let cfg = Config::default();
        let vm = run(
            vec![Op::LoadI { dst: 5, value: 99 }, Op::Mov { dst: 0, src: 5 }],
            &cfg,
        );
        assert_eq!(vm.r(0).as_int(), 99);
        assert_eq!(vm.r(5).as_int(), 99);
    }

    #[test]
    fn jmp_skips_instructions() {
        let cfg = Config::default();
        let vm = run(
            vec![
                Op::LoadI { dst: 0, value: 1 },
                Op::Jmp { target: 3 },
                Op::LoadI { dst: 0, value: 999 }, // skipped
                Op::LoadI { dst: 1, value: 2 },
            ],
            &cfg,
        );
        assert_eq!(vm.r(0).as_int(), 1);
        assert_eq!(vm.r(1).as_int(), 2);
    }

    /// Op::JmpF is misnamed — semantically it is "jump if true". Pin that down.
    #[test]
    fn jmpf_jumps_when_cond_is_true() {
        let cfg = Config::default();
        let vm = run(
            vec![
                Op::LoadI { dst: 0, value: 1 }, // truthy
                Op::JmpF { cond: 0, target: 3 },
                Op::LoadI { dst: 1, value: 999 }, // skipped
                Op::LoadI { dst: 2, value: 7 },
            ],
            &cfg,
        );
        assert_eq!(vm.r(1).as_int(), 0);
        assert_eq!(vm.r(2).as_int(), 7);
    }

    #[test]
    fn jmpf_falls_through_when_cond_is_false() {
        let cfg = Config::default();
        let vm = run(
            vec![
                Op::LoadI { dst: 0, value: 0 },
                Op::JmpF { cond: 0, target: 3 },
                Op::LoadI { dst: 1, value: 11 },
                Op::LoadI { dst: 2, value: 22 },
            ],
            &cfg,
        );
        assert_eq!(vm.r(1).as_int(), 11);
        assert_eq!(vm.r(2).as_int(), 22);
    }

    #[test]
    fn push_pop_roundtrip() {
        let cfg = Config::default();
        let vm = run(
            vec![
                Op::LoadI { dst: 0, value: 10 },
                Op::LoadI { dst: 1, value: 20 },
                Op::Push { src: 0 },
                Op::Push { src: 1 },
                Op::LoadI { dst: 0, value: 0 },
                Op::LoadI { dst: 1, value: 0 },
                Op::Pop { dst: 1 },
                Op::Pop { dst: 0 },
            ],
            &cfg,
        );
        assert_eq!(vm.r(0).as_int(), 10);
        assert_eq!(vm.r(1).as_int(), 20);
    }

    #[test]
    fn casts() {
        let cfg = Config::default();
        let mut vm = Vm::new(&cfg);
        vm.globals.push(Value::from(3.7_f64));
        vm.bytecode = vec![
            Op::LoadI { dst: 0, value: 5 },
            Op::CastToDouble { dst: 1, src: 0 },
            Op::LoadG { dst: 2, idx: 0 },
            Op::CastToInt { dst: 3, src: 2 },
            Op::CastToBool { dst: 4, src: 0 },
            Op::LoadI { dst: 5, value: 0 },
            Op::CastToBool { dst: 6, src: 5 },
        ];
        vm.run().unwrap();
        assert_eq!(vm.r(1).as_f64(), 5.0);
        assert_eq!(vm.r(3).as_int(), 3);
        assert!(vm.r(4).as_bool());
        assert!(!vm.r(6).as_bool());
    }

    #[test]
    fn call_ret_roundtrip() {
        let cfg = Config::default();
        // Callee at pc=4 sets r1 = 7, returns. Caller calls it, then writes r2 = 99.
        let vm = run(
            vec![
                Op::Call { func: 4 },
                Op::LoadI { dst: 2, value: 99 },
                Op::Jmp { target: 6 }, // skip over callee
                Op::Nop,               // padding so pc=4 is the callee
                Op::LoadI { dst: 1, value: 7 },
                Op::Ret,
                Op::Nop, // jump landing
            ],
            &cfg,
        );
        assert_eq!(vm.r(1).as_int(), 7);
        assert_eq!(vm.r(2).as_int(), 99);
    }

    #[test]
    fn beq() {
        let cfg = Config::default();
        let vm = run(
            vec![
                Op::LoadI { dst: 0, value: 1 },
                Op::LoadI { dst: 1, value: 1 },
                Op::LoadI { dst: 2, value: 0 },
                Op::BEq {
                    dst: 3,
                    lhs: 0,
                    rhs: 1,
                },
                Op::BEq {
                    dst: 4,
                    lhs: 0,
                    rhs: 2,
                },
            ],
            &cfg,
        );
        assert!(vm.r(3).as_bool());
        assert!(!vm.r(4).as_bool());
    }
}
