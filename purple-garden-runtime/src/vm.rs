use crate::{
    Anomaly, BuiltinFn, REGISTER_COUNT, Value,
    gc::{AllocType, Gc},
    op::Op,
};
use std::{alloc::Layout, ffi::c_void};

#[derive(Clone, Copy, Debug, Default)]
pub struct VmConfig {
    pub backtrace: bool,
    pub no_gc: bool,
}

/// Return address of the synthetic root call frame pushed in [`Vm::new`].
/// Chosen so that after the dispatcher's unconditional `pc += 1` the program
/// counter lands at `usize::MAX`; never less than the bytecode length, so the
/// run loop exits. `MAX - 1` (not `MAX`) keeps that `+ 1` from overflowing in
/// debug builds.
const ROOT_RETURN_ADDR: usize = usize::MAX - 1;

type CollectFn = fn(&mut Vm);

#[derive(Default, Debug)]
pub struct CallFrame {
    pub return_to: usize,
    /// Snapshot of [`Vm::spilled`].`len()` at call entry. Used by the debug
    /// check on [`Op::Ret`] to catch bytecode that leaves the spill stack
    /// unbalanced across a call.
    #[cfg(debug_assertions)]
    pub spilled_depth: usize,
}

/// Source-location side table for a compiled program.
#[derive(Debug, Default)]
pub struct DebugInfo {
    /// `pc_to_span[pc]` is the byte offset into the source of the AST
    /// node that produced the op at `pc`.
    pc_to_span: Box<[u32]>,
}

impl DebugInfo {
    #[must_use]
    pub fn new(pc_to_span: Box<[u32]>) -> Self {
        Self { pc_to_span }
    }

    /// Source byte offset for `pc`, or 0 if `pc` is out of range.
    #[inline]
    #[must_use]
    pub fn span_at(&self, pc: usize) -> u32 {
        self.pc_to_span.get(pc).copied().unwrap_or(0)
    }
}

pub unsafe extern "C" fn syscall_unimplemented(vm: *mut c_void) {
    let vm = unsafe { &mut *vm.cast::<Vm>() };
    vm.trap(Anomaly::InvalidSyscall { pc: vm.pc });
}

#[repr(C)]
#[derive(Debug)]
pub struct Vm {
    r: [Value; REGISTER_COUNT],
    pub pc: usize,

    frames: Vec<CallFrame>,
    /// a stack to keep values alive across recursive function invocations
    spilled: Vec<Value>,

    pub bytecode: Vec<Op>,
    pub globals: Vec<Value>,
    pub gc: Gc,
    pub const_strings: Vec<Box<[u8]>>,

    /// backtrace holds a list of indexes into the bytecode, pointing to the definition site of the
    /// function the virtual machine currently executes in, this behaviour only occurs if
    /// --backtrace was passed as an option to the interpreter
    pub backtrace: Vec<usize>,

    /// A trap raised by a syscall via [`Vm::trap`]. Checked at each [`Op::Ret`]
    /// so the `Op::Sys` hot path stays branch-free.
    pub pending_trap: Option<Anomaly>,

    config: VmConfig,
    /// Called when allocation wants to run a collection pass.
    collect_fn: CollectFn,
}

/// trap in the vm; return Err(<anomaly>) if expr == true
#[allow(unused)]
macro_rules! trap_if {
    ($condition:expr, $anomaly:expr) => {
        if std::hint::unlikely($condition) {
            return Err($anomaly);
        }
    };
}

impl Vm {
    #[must_use]
    pub fn new(config: VmConfig) -> Self {
        let mut frames = Vec::with_capacity(64);
        // Synthetic root frame: the VM enters the entry function directly, so
        // its trailing Op::Ret needs a frame to pop. Popping it ends the run
        // (see ROOT_RETURN_ADDR) and drains any pending trap.
        frames.push(CallFrame {
            return_to: ROOT_RETURN_ADDR,
            #[cfg(debug_assertions)]
            spilled_depth: 0,
        });
        let collect = if config.no_gc {
            Self::collect_noop
        } else {
            Self::collect
        };

        Self {
            r: [const { Value(0) }; REGISTER_COUNT],
            frames,
            pc: 0,
            bytecode: Vec::new(),
            globals: Vec::new(),
            gc: Gc::new(),
            const_strings: Vec::new(),
            backtrace: Vec::new(),
            spilled: Vec::with_capacity(4096),
            pending_trap: None,
            config,
            collect_fn: collect,
        }
    }

    fn collect(&mut self) {
        self.gc.collect();
    }

    fn collect_noop(&mut self) {}

    pub fn try_alloc(
        &mut self,
        alloc_type: AllocType,
        layout: Layout,
    ) -> Option<std::ptr::NonNull<u8>> {
        if let Some(ptr) = self.gc.alloc_fast(alloc_type, layout) {
            return Some(ptr);
        }

        self.alloc_slow(alloc_type, layout)
    }

    #[cold]
    fn alloc_slow(
        &mut self,
        alloc_type: AllocType,
        layout: Layout,
    ) -> Option<std::ptr::NonNull<u8>> {
        (self.collect_fn)(self);

        if let Some(ptr) = self.gc.alloc_fast(alloc_type, layout) {
            return Some(ptr);
        }

        self.gc.grow(layout).ok()?;
        self.gc.alloc_fast(alloc_type, layout)
    }

    pub fn alloc(&mut self, alloc_type: AllocType, layout: Layout) -> std::ptr::NonNull<u8> {
        self.try_alloc(alloc_type, layout)
            .expect("GC allocation failed")
    }

    pub fn new_string(&mut self, s: String) -> Value {
        let bytes = s.as_bytes();
        let len_size = std::mem::size_of::<usize>();
        let layout = Layout::from_size_align(len_size + bytes.len(), std::mem::align_of::<usize>())
            .expect("string allocation layout");
        let payload = self.alloc(AllocType::String, layout).as_ptr();

        unsafe {
            (payload as *mut usize).write(bytes.len());
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), payload.add(len_size), bytes.len());
        }

        Value::from_ptr(payload)
    }

    pub fn new_const_string(&mut self, s: String) -> Value {
        let len_size = std::mem::size_of::<usize>();
        let mut payload = vec![0u8; len_size + s.len()].into_boxed_slice();
        unsafe {
            (payload.as_mut_ptr() as *mut usize).write(s.len());
            std::ptr::copy_nonoverlapping(s.as_ptr(), payload.as_mut_ptr().add(len_size), s.len());
        }
        let value = Value::from_ptr(payload.as_mut_ptr());
        self.const_strings.push(payload);
        value
    }

    pub fn run(&mut self, syscalls: &[BuiltinFn]) -> Result<(), Anomaly> {
        let regs = self.r.as_mut_ptr();
        let instructions = self.bytecode.as_mut_ptr();
        let instructions_len = self.bytecode.len();
        let globals = self.globals.as_mut_ptr();
        let syscalls = syscalls.as_ptr();

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
                Op::IAddI { dst, lhs, imm } => unsafe {
                    let l = r!(lhs).as_int();
                    r_mut!(dst) = Value::from(l + imm as i64);
                },
                Op::ISub { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_int();
                    let r = r!(rhs).as_int();
                    r_mut!(dst) = Value::from(l - r);
                },
                Op::ISubI { dst, lhs, imm } => unsafe {
                    let l = r!(lhs).as_int();
                    r_mut!(dst) = Value::from(l - imm as i64);
                },
                Op::IMul { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_int();
                    let r = r!(rhs).as_int();
                    r_mut!(dst) = Value::from(l * r);
                },
                Op::IMulI { dst, lhs, imm } => unsafe {
                    let l = r!(lhs).as_int();
                    r_mut!(dst) = Value::from(l * imm as i64);
                },
                Op::IDiv { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_int();
                    let r = r!(rhs).as_int();
                    trap_if!(r == 0, Anomaly::DivisionByZero { pc });
                    r_mut!(dst) = Value::from(l / r);
                },
                Op::IDivI { dst, lhs, imm } => unsafe {
                    let imm = imm as i64;
                    trap_if!(imm == 0, Anomaly::DivisionByZero { pc });
                    let l = r!(lhs).as_int();
                    r_mut!(dst) = Value::from(l / imm);
                },
                Op::IMod { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_int();
                    let r = r!(rhs).as_int();
                    trap_if!(r == 0, Anomaly::DivisionByZero { pc });
                    r_mut!(dst) = Value::from(l % r);
                },
                Op::IModI { dst, lhs, imm } => unsafe {
                    let imm = imm as i64;
                    trap_if!(imm == 0, Anomaly::DivisionByZero { pc });
                    let l = r!(lhs).as_int();
                    r_mut!(dst) = Value::from(l % imm);
                },
                Op::IEq { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_int();
                    let r = r!(rhs).as_int();
                    r_mut!(dst) = Value::from(l == r);
                },
                Op::IEqI { dst, lhs, imm } => unsafe {
                    let l = r!(lhs).as_int();
                    r_mut!(dst) = Value::from(l == imm as i64);
                },
                Op::IGt { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_int();
                    let r = r!(rhs).as_int();
                    r_mut!(dst) = Value::from(l > r);
                },
                Op::IGtI { dst, lhs, imm } => unsafe {
                    let l = r!(lhs).as_int();
                    r_mut!(dst) = Value::from(l > imm as i64);
                },
                Op::ILt { dst, lhs, rhs } => unsafe {
                    let l = r!(lhs).as_int();
                    let r = r!(rhs).as_int();
                    r_mut!(dst) = Value::from(l < r);
                },
                Op::ILtI { dst, lhs, imm } => unsafe {
                    let l = r!(lhs).as_int();
                    r_mut!(dst) = Value::from(l < imm as i64);
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
                    if std::hint::unlikely(self.config.backtrace) {
                        self.backtrace.push(func as usize);
                    }
                    pc = func as usize;
                    continue;
                }
                Op::JmpT { target, cond } => unsafe {
                    if r!(cond).as_bool() {
                        pc = target as usize;
                        continue;
                    }
                },
                Op::JmpF { target, cond } => unsafe {
                    if !r!(cond).as_bool() {
                        pc = target as usize;
                        continue;
                    }
                },
                Op::JmpEqI { lhs, imm, target } => unsafe {
                    if r!(lhs).as_int() == imm as i64 {
                        pc = target as usize;
                        continue;
                    }
                },
                Op::JmpNeI { lhs, imm, target } => unsafe {
                    if r!(lhs).as_int() != imm as i64 {
                        pc = target as usize;
                        continue;
                    }
                },
                Op::Call { func } => {
                    if std::hint::unlikely(self.config.backtrace) {
                        self.backtrace.push(func as usize);
                    }

                    self.frames.push(CallFrame {
                        return_to: pc,
                        #[cfg(debug_assertions)]
                        spilled_depth: self.spilled.len(),
                    });
                    pc = func as usize;
                    continue;
                }
                Op::Sys { idx } => unsafe {
                    #[cfg(debug_assertions)]
                    let pre_sys: [Value; REGISTER_COUNT] = self.r;

                    // Publish the current pc before the call: a syscall that
                    // traps reads vm.pc to locate itself, but the loop only
                    // writes self.pc on exit, so without this the trap would
                    // carry a stale pc and render at the wrong source span.
                    self.pc = pc;
                    (*syscalls.add(idx as usize))((self as *mut Vm).cast());

                    #[cfg(debug_assertions)]
                    for (i, pre) in pre_sys.iter().enumerate().skip(1) {
                        debug_assert_eq!(
                            pre.0, self.r[i].0,
                            "syscall idx={idx} wrote r{i}; convention only permits writes to r0"
                        );
                    }
                },
                Op::Ret => {
                    if std::hint::unlikely(self.config.backtrace) {
                        self.backtrace.pop();
                    }

                    // PERF: fully replacing the pop with just an access and a length truncation?

                    // The synthetic root frame from Vm::new guarantees the
                    // stack is never empty here, so the pop always yields a
                    // frame.
                    let frame = unsafe { self.frames.pop().unwrap_unchecked() };

                    // See Op::Push: every function must leave the spill
                    // stack at the depth it found it. Catches arg-shuffle
                    // cycle paths or caller-save spills that forgot to
                    // pair their Pops.
                    #[cfg(debug_assertions)]
                    debug_assert_eq!(
                        frame.spilled_depth,
                        self.spilled.len(),
                        "function returning to pc={} left vm.spilled unbalanced (entered at depth {}, exiting at depth {})",
                        frame.return_to,
                        frame.spilled_depth,
                        self.spilled.len(),
                    );
                    if std::hint::unlikely(self.pending_trap.is_some()) {
                        return Err(self.pending_trap.take().unwrap());
                    }
                    pc = frame.return_to;
                }
                Op::Push { src } => unsafe {
                    self.spilled.push(*r!(src));
                },
                Op::Push2 { a, b } => unsafe {
                    self.spilled.push(*r!(a));
                    self.spilled.push(*r!(b));
                },
                Op::Push3 { a, b, c } => unsafe {
                    self.spilled.push(*r!(a));
                    self.spilled.push(*r!(b));
                    self.spilled.push(*r!(c));
                },
                Op::Pop { dst } => unsafe {
                    r_mut!(dst) = self.spilled.pop().unwrap();
                },
                Op::Pop2 { a, b } => unsafe {
                    r_mut!(a) = self.spilled.pop().unwrap();
                    r_mut!(b) = self.spilled.pop().unwrap();
                },
                Op::Pop3 { a, b, c } => unsafe {
                    r_mut!(a) = self.spilled.pop().unwrap();
                    r_mut!(b) = self.spilled.pop().unwrap();
                    r_mut!(c) = self.spilled.pop().unwrap();
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
                Op::Alloc {
                    dst,
                    kind,
                    size,
                    align,
                } => unsafe {
                    r_mut!(dst) = Value::from_ptr(
                        self.alloc(
                            kind,
                            Layout::from_size_align_unchecked(size as usize, align as usize),
                        )
                        .as_ptr(),
                    );
                },
                Op::Store { base, offset, src } => unsafe {
                    (r!(base).as_ptr::<u8>().add(offset as usize) as *mut Value).write(*r!(src));
                },
                Op::Load { dst, base, offset } => unsafe {
                    r_mut!(dst) = *(r!(base).as_ptr::<u8>().add(offset as usize) as *const Value);
                },
                Op::AddrOf { dst, base, offset } => unsafe {
                    r_mut!(dst) = Value::from_ptr(r!(base).as_ptr::<u8>().add(offset as usize));
                },
                Op::Nop => {}
            }

            pc += 1;
        }

        self.pc = pc;

        Ok(())
    }

    /// Raise a trap from a syscall body. Checked at the next [`Op::Ret`].
    #[inline(always)]
    pub fn trap(&mut self, anomaly: Anomaly) {
        self.pending_trap = Some(anomaly);
    }

    #[inline(always)]
    pub fn take_trap(&mut self) -> Option<Anomaly> {
        self.pending_trap.take()
    }

    #[inline(always)]
    /// access register [idx] by indexing [`vm::r`]
    #[must_use]
    pub fn r(&self, idx: usize) -> &Value {
        unsafe { &*self.r.as_ptr().add(idx) }
    }

    #[inline(always)]
    /// access register [idx] mutably by indexing [`vm::r`]
    pub fn r_mut(&mut self, idx: usize) -> &mut Value {
        unsafe { &mut *self.r.as_mut_ptr().add(idx) }
    }
}

#[cfg(test)]
mod ops {
    use super::*;

    fn run(bytecode: Vec<Op>) -> Vm {
        let mut vm = Vm::new(VmConfig::default());
        vm.bytecode = bytecode;
        vm.run(&[]).expect("vm run failed");
        vm
    }

    fn run_err(bytecode: Vec<Op>) -> Anomaly {
        let mut vm = Vm::new(VmConfig::default());
        vm.bytecode = bytecode;
        vm.run(&[]).expect_err("vm run unexpectedly succeeded")
    }

    #[test]
    fn iadd() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: 7 },
            Op::LoadI { dst: 1, value: 35 },
            Op::IAdd {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ]);
        assert_eq!(vm.r(2).as_int(), 42);
    }

    #[test]
    fn isub() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: 50 },
            Op::LoadI { dst: 1, value: 8 },
            Op::ISub {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ]);
        assert_eq!(vm.r(2).as_int(), 42);
    }

    #[test]
    fn imul() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: 6 },
            Op::LoadI { dst: 1, value: 7 },
            Op::IMul {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ]);
        assert_eq!(vm.r(2).as_int(), 42);
    }

    #[test]
    fn idiv() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: 84 },
            Op::LoadI { dst: 1, value: 2 },
            Op::IDiv {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ]);
        assert_eq!(vm.r(2).as_int(), 42);
    }

    #[test]
    fn idiv_by_zero_traps() {
        let err = run_err(vec![
            Op::LoadI { dst: 0, value: 1 },
            Op::LoadI { dst: 1, value: 0 },
            Op::IDiv {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ]);
        assert!(matches!(err, Anomaly::DivisionByZero { .. }));
    }

    #[test]
    fn imod() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: 43 },
            Op::LoadI { dst: 1, value: 5 },
            Op::IMod {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ]);
        assert_eq!(vm.r(2).as_int(), 3);
    }

    #[test]
    fn imod_i() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: -43 },
            Op::IModI {
                dst: 1,
                lhs: 0,
                imm: 5,
            },
        ]);
        assert_eq!(vm.r(1).as_int(), -3);
    }

    #[test]
    fn imod_by_zero_traps() {
        let err = run_err(vec![
            Op::LoadI { dst: 0, value: 1 },
            Op::LoadI { dst: 1, value: 0 },
            Op::IMod {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ]);
        assert!(matches!(err, Anomaly::DivisionByZero { .. }));
    }

    #[test]
    fn ddiv_by_zero_traps() {
        let mut vm = Vm::new(VmConfig::default());
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
        let err = vm.run(&[]).expect_err("ddiv by zero should trap");
        assert!(matches!(err, Anomaly::DivisionByZero { .. }));
    }

    #[test]
    fn int_compare() {
        let vm = run(vec![
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
        ]);
        assert!(!vm.r(2).as_bool());
        assert!(vm.r(3).as_bool());
        assert!(!vm.r(4).as_bool());
    }

    #[test]
    fn int_compare_immediate() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: 42 },
            Op::IEqI {
                dst: 1,
                lhs: 0,
                imm: 42,
            },
            Op::IEqI {
                dst: 2,
                lhs: 0,
                imm: 7,
            },
        ]);
        assert!(vm.r(1).as_bool());
        assert!(!vm.r(2).as_bool());
    }

    #[test]
    fn double_arith() {
        let mut vm = Vm::new(VmConfig::default());
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
        vm.run(&[]).unwrap();
        assert_eq!(vm.r(2).as_f64(), 4.0);
        assert_eq!(vm.r(3).as_f64(), 1.0);
        assert_eq!(vm.r(4).as_f64(), 3.75);
        assert_eq!(vm.r(5).as_f64(), 2.5 / 1.5);
    }

    #[test]
    fn mov() {
        let vm = run(vec![
            Op::LoadI { dst: 5, value: 99 },
            Op::Mov { dst: 0, src: 5 },
        ]);
        assert_eq!(vm.r(0).as_int(), 99);
        assert_eq!(vm.r(5).as_int(), 99);
    }

    #[test]
    fn alloc_store_writes_payload_offset() {
        let vm = run(vec![
            Op::Alloc {
                dst: 0,
                kind: AllocType::Record,
                size: 16,
                align: 8,
            },
            Op::LoadI { dst: 1, value: 42 },
            Op::Store {
                base: 0,
                offset: 8,
                src: 1,
            },
        ]);

        let payload = vm.r(0).as_ptr::<u8>();
        let stored = unsafe { *(payload.add(8) as *const Value) };
        assert_eq!(stored.as_int(), 42);
    }

    #[test]
    fn alloc_store_load_roundtrip() {
        let vm = run(vec![
            Op::Alloc {
                dst: 0,
                kind: AllocType::Record,
                size: 16,
                align: 8,
            },
            Op::LoadI { dst: 1, value: 42 },
            Op::Store {
                base: 0,
                offset: 8,
                src: 1,
            },
            Op::Load {
                dst: 2,
                base: 0,
                offset: 8,
            },
        ]);

        assert_eq!(vm.r(2).as_int(), 42);
    }

    #[test]
    fn addrof_points_at_payload_offset() {
        let vm = run(vec![
            Op::Alloc {
                dst: 0,
                kind: AllocType::Record,
                size: 16,
                align: 8,
            },
            Op::LoadI { dst: 1, value: 42 },
            Op::Store {
                base: 0,
                offset: 8,
                src: 1,
            },
            Op::AddrOf {
                dst: 2,
                base: 0,
                offset: 8,
            },
            Op::Load {
                dst: 3,
                base: 2,
                offset: 0,
            },
        ]);

        assert_eq!(vm.r(3).as_int(), 42);
    }

    #[test]
    fn jmp_skips_instructions() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: 1 },
            Op::Jmp { target: 3 },
            Op::LoadI { dst: 0, value: 999 }, // skipped
            Op::LoadI { dst: 1, value: 2 },
        ]);
        assert_eq!(vm.r(0).as_int(), 1);
        assert_eq!(vm.r(1).as_int(), 2);
    }

    #[test]
    fn jmpt_jumps_when_cond_is_true() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: 1 }, // truthy
            Op::JmpT { cond: 0, target: 3 },
            Op::LoadI { dst: 1, value: 999 }, // skipped
            Op::LoadI { dst: 2, value: 7 },
        ]);
        assert_eq!(vm.r(1).as_int(), 0);
        assert_eq!(vm.r(2).as_int(), 7);
    }

    #[test]
    fn jmpt_falls_through_when_cond_is_false() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: 0 },
            Op::JmpT { cond: 0, target: 3 },
            Op::LoadI { dst: 1, value: 11 },
            Op::LoadI { dst: 2, value: 22 },
        ]);
        assert_eq!(vm.r(1).as_int(), 11);
        assert_eq!(vm.r(2).as_int(), 22);
    }

    #[test]
    fn push_pop_roundtrip() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: 10 },
            Op::LoadI { dst: 1, value: 20 },
            Op::Push { src: 0 },
            Op::Push { src: 1 },
            Op::LoadI { dst: 0, value: 0 },
            Op::LoadI { dst: 1, value: 0 },
            Op::Pop { dst: 1 },
            Op::Pop { dst: 0 },
        ]);
        assert_eq!(vm.r(0).as_int(), 10);
        assert_eq!(vm.r(1).as_int(), 20);
    }

    #[test]
    fn packed_push_pop_roundtrip() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: 10 },
            Op::LoadI { dst: 1, value: 20 },
            Op::LoadI { dst: 2, value: 30 },
            Op::LoadI { dst: 3, value: 40 },
            Op::LoadI { dst: 4, value: 50 },
            Op::Push3 { a: 0, b: 1, c: 2 },
            Op::Push2 { a: 3, b: 4 },
            Op::LoadI { dst: 0, value: 0 },
            Op::LoadI { dst: 1, value: 0 },
            Op::LoadI { dst: 2, value: 0 },
            Op::LoadI { dst: 3, value: 0 },
            Op::LoadI { dst: 4, value: 0 },
            Op::Pop3 { a: 4, b: 3, c: 2 },
            Op::Pop2 { a: 1, b: 0 },
        ]);
        assert_eq!(vm.r(0).as_int(), 10);
        assert_eq!(vm.r(1).as_int(), 20);
        assert_eq!(vm.r(2).as_int(), 30);
        assert_eq!(vm.r(3).as_int(), 40);
        assert_eq!(vm.r(4).as_int(), 50);
    }

    #[test]
    fn casts() {
        let mut vm = Vm::new(VmConfig::default());
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
        vm.run(&[]).unwrap();
        assert_eq!(vm.r(1).as_f64(), 5.0);
        assert_eq!(vm.r(3).as_int(), 3);
        assert!(vm.r(4).as_bool());
        assert!(!vm.r(6).as_bool());
    }

    #[test]
    fn call_ret_roundtrip() {
        // Callee at pc=4 sets r1 = 7, returns. Caller calls it, then writes r2 = 99.
        let vm = run(vec![
            Op::Call { func: 4 },
            Op::LoadI { dst: 2, value: 99 },
            Op::Jmp { target: 6 }, // skip over callee
            Op::Nop,               // padding so pc=4 is the callee
            Op::LoadI { dst: 1, value: 7 },
            Op::Ret,
            Op::Nop, // jump landing
        ]);
        assert_eq!(vm.r(1).as_int(), 7);
        assert_eq!(vm.r(2).as_int(), 99);
    }

    #[test]
    fn beq() {
        let vm = run(vec![
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
        ]);
        assert!(vm.r(3).as_bool());
        assert!(!vm.r(4).as_bool());
    }
}
