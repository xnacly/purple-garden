use crate::{
    Anomaly, BuiltinFn, REGISTER_COUNT, Value,
    gc::Gc,
    op::{LookupTable, SwitchKind, SwitchTable, Tag, packed::Packed},
    string,
};
use purple_garden_allocators::{page::PageAlloc, stack::StackAlloc};
use std::{
    alloc::{Allocator, Layout},
    mem::MaybeUninit,
};

mod alloc;
mod config;
mod handlers;

pub use alloc::VmAlloc;
pub use config::{VmConfig, frames_in};

type CollectFn<'vm> = fn(&mut Vm<'vm>);

#[derive(Default, Debug, Clone, Copy)]
pub struct CallFrame {
    pub return_to: usize,
    /// Snapshot of [`Vm::spilled`].`len()` at call entry. Used by the debug
    /// check on [`Op::Ret`](crate::op::Op::Ret) to catch bytecode that leaves the spill stack
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

/// Bytes of spill stack [`Vm::run`] keeps on its own stack frame.
pub const SPILL_STACK: usize = 256 << 10;

/// Where JIT code reads [`Vm::native_stack_limit`] from the vm pointer.
pub const NATIVE_STACK_LIMIT_OFFSET: usize = std::mem::offset_of!(Vm, native_stack_limit);

/// Where [`crate::enter_native`] and [`crate::jit_abort`] keep [`Vm::native_entry_rsp`].
pub const NATIVE_ENTRY_RSP_OFFSET: usize = std::mem::offset_of!(Vm, native_entry_rsp);

// Native code addresses register i at `[vm + 8 * i]`.
const _: () = assert!(std::mem::offset_of!(Vm, r) == 0 && size_of::<Value>() == 8);

#[repr(C)]
#[derive(Debug)]
pub struct Vm<'vm> {
    r: [Value; REGISTER_COUNT],
    pub pc: usize,

    frames: Box<[CallFrame]>,
    frame_depth: usize,

    pub bytecode: Box<[Packed]>,
    pub globals: Box<[Value]>,
    /// The tables [`Op::Switch`](crate::op::Op::Switch) indexes.
    pub switch_tables: Box<[SwitchTable]>,
    /// The tables [`Op::Lookup`](crate::op::Op::Lookup) indexes.
    pub lookup_tables: Box<[LookupTable]>,
    pub gc: Gc<VmAlloc<'vm>>,

    /// Backing storage for string constants emitted by the compiler.
    pub const_pool: Box<[u32]>,
    /// Every string, constants included, see [`Vm::new_string_from_str`].
    pub strings: string::StrTable,

    /// backtrace holds a list of indexes into the bytecode, pointing to the definition site of the
    /// function the virtual machine currently executes in, this behaviour only occurs if
    /// --backtrace was passed as an option to the interpreter
    pub backtrace: Vec<usize>,

    /// A trap raised by a syscall via [`Vm::trap`]. Checked at each [`Op::Ret`](crate::op::Op::Ret)
    /// so the `Op::Sys` hot path stays branch-free.
    pub pending_trap: Option<Anomaly>,
    /// Native code traps instead of growing its stack below this, see
    /// [`NATIVE_STACK_LIMIT_OFFSET`]. 0 disables the check.
    pub native_stack_limit: usize,
    /// The stack slot holding the return address of [`crate::enter_native`]'s call into native
    /// code, [`crate::jit_abort`] returns through it. Native code is never reentered, so one slot
    /// suffices.
    pub native_entry_rsp: usize,
    /// The box every None points to, see [`crate::Type::Option`]. Singleton so the vm doesnt
    /// allocate useless Option::None boxes for all none(d?) optionals
    pub none: Value,

    pub config: VmConfig,
    /// The syscalls and spill stack of the running [`Vm::run`], only valid while it runs.
    syscalls: *const BuiltinFn,
    spilled: *const StackAlloc<'static>,
    /// Called when allocation wants to run a collection pass.
    collect_fn: CollectFn<'vm>,
}

impl Vm<'static> {
    #[must_use]
    pub fn new(config: VmConfig) -> Self {
        Self::new_in(config, PageAlloc {})
    }
}

impl<'vm> Vm<'vm> {
    /// The GC takes its pages from `alloc`.
    #[must_use]
    pub fn new_in(config: VmConfig, alloc: impl Allocator + 'vm) -> Self {
        // Zeroed pages come untouched from the OS, so only the depth a program
        // reaches becomes resident. All zero is a valid, empty CallFrame.
        let mut frames = unsafe {
            Box::<[CallFrame]>::new_zeroed_slice(frames_in(config.stack_size).saturating_add(1))
                .assume_init()
        };
        frames[0] = Self::root_frame();
        let collect = if config.no_gc {
            Self::collect_noop
        } else {
            Self::collect
        };

        let mut vm = Self {
            r: [const { Value(0) }; REGISTER_COUNT],
            frames,
            frame_depth: 1,
            pc: 0,
            bytecode: Box::default(),
            globals: Box::default(),
            switch_tables: Box::default(),
            lookup_tables: Box::default(),
            gc: Gc::new_in(VmAlloc(Box::new(alloc))),
            const_pool: Box::new([]),
            strings: string::StrTable::default(),
            backtrace: Vec::new(),
            pending_trap: None,
            native_stack_limit: 0,
            native_entry_rsp: 0,
            none: Value::UNDEF,
            config,
            syscalls: std::ptr::null(),
            spilled: std::ptr::null(),
            collect_fn: collect,
        };
        vm.none = vm.option_box(0, Value::UNDEF);
        vm
    }

    fn root_frame() -> CallFrame {
        // The VM enters a top-level function directly, so its trailing Op::Ret
        // needs a frame to pop. Popping this synthetic frame lands on the
        // trailing Op::Halt and drains any pending trap. `return_to` is a
        // placeholder; `run` patches it once the bytecode length is known.
        CallFrame {
            return_to: usize::MAX,
            #[cfg(debug_assertions)]
            spilled_depth: 0,
        }
    }

    /// Prepare the VM for a new execution
    ///
    /// [`Self::run`] consumes the synthetic root frame when the entered function returns. Call this
    /// before every independent invocation; it also discards call state left behind by a trapped
    /// run.
    pub fn reset(&mut self) {
        // poison le well in debugging so we know whats going on
        #[cfg(debug_assertions)]
        self.r.fill(Value(0xDEAD_AFFE_DEAD_AFFE));
        self.pc = usize::MAX;
        self.frames[0] = Self::root_frame();
        self.frame_depth = 1;
        self.backtrace.clear();
        self.pending_trap = None;
        // Room for builtins and the trap path once native code hits the limit.
        const MARGIN: usize = 256 << 10;
        self.native_stack_limit =
            purple_garden_shared::stack::thread_stack_low().map_or(0, |low| low + MARGIN);
    }

    pub fn run<const BACKTRACE: bool>(&mut self, syscalls: &[BuiltinFn]) -> Result<(), Anomaly> {
        let Some(last) = self.bytecode.last() else {
            return Ok(());
        };
        debug_assert!(
            last.tag() == Tag::Halt,
            "bytecode must end in Op::Halt (bc::Cc::finalize emits it)"
        );
        let halt = self.bytecode.len() - 1;
        // wrapping_sub covers bytecode that is only a Halt.
        self.frames[0].return_to = halt.wrapping_sub(1);

        // Spills only live while this loop runs, so they live on its stack.
        let mut spill_stack = [MaybeUninit::<u8>::uninit(); SPILL_STACK];
        let spilled = StackAlloc::new(&mut spill_stack);

        let pc = self.pc;
        if pc > halt {
            return Ok(());
        }

        self.syscalls = syscalls.as_ptr();
        self.spilled = (&raw const spilled).cast();
        let code = self.bytecode.as_ptr();
        // SAFETY: bc ends in Halt and only jumps within bytecode, the handlers uphold the rest
        self.pc = unsafe { handlers::enter::<BACKTRACE>((self as *mut Self).cast(), code, pc)? };

        Ok(())
    }

    /// Record a trap for a jit helper to leave native code with through `jit_abort`, read back once
    /// `enter_native` returns.
    #[inline(always)]
    pub fn trap(&mut self, anomaly: Anomaly) {
        self.pending_trap = Some(anomaly);
    }

    #[inline(always)]
    pub fn take_trap(&mut self) -> Option<Anomaly> {
        self.pending_trap.take()
    }

    /// access register [idx] by indexing [`vm::r`]
    ///
    /// # Safety
    ///
    /// `idx < REGISTER_COUNT`.
    #[inline(always)]
    #[must_use]
    pub unsafe fn r(&self, idx: usize) -> &Value {
        // SAFETY: caller upholds # Safety
        unsafe { &*self.r.as_ptr().add(idx) }
    }

    /// access register [idx] mutably by indexing [`vm::r`]
    ///
    /// # Safety
    ///
    /// `idx < REGISTER_COUNT`.
    #[inline(always)]
    pub unsafe fn r_mut(&mut self, idx: usize) -> &mut Value {
        // SAFETY: caller upholds # Safety
        unsafe { &mut *self.r.as_mut_ptr().add(idx) }
    }
}

#[cfg(test)]
mod ops {
    use super::*;
    use crate::{gc::AllocType, op::Op};

    fn packed<const N: usize>(ops: [Op; N]) -> Box<[Packed]> {
        ops.map(Op::encode).into()
    }

    /// Every string created before the intern table rehashes must still be
    /// found after it.
    #[test]
    fn strings_stay_interned_across_table_growth() {
        let mut vm = Vm::new(VmConfig::default());
        let first: Vec<Value> = (0..200).map(|i| vm.new_string(i.to_string())).collect();
        for (i, v) in first.iter().enumerate() {
            assert_eq!(vm.new_string_from_str(&i.to_string()), *v);
        }
    }

    fn run(mut bytecode: Vec<Op>) -> Vm<'static> {
        bytecode.push(Op::Halt);
        let mut vm = Vm::new(VmConfig::default());
        vm.bytecode = bytecode.into_iter().map(Op::encode).collect();
        vm.run::<false>(&[]).expect("vm run failed");
        vm
    }

    fn run_err(mut bytecode: Vec<Op>) -> Anomaly {
        bytecode.push(Op::Halt);
        let mut vm = Vm::new(VmConfig::default());
        vm.bytecode = bytecode.into_iter().map(Op::encode).collect();
        vm.run::<false>(&[])
            .expect_err("vm run unexpectedly succeeded")
    }

    #[test]
    fn prepare_run_reinstalls_root_frame_after_return() {
        let mut vm = Vm::new(VmConfig::default());
        vm.bytecode = packed([Op::LoadI { dst: 0, value: 42 }, Op::Ret, Op::Halt]);

        vm.reset();
        vm.pc = 0;
        vm.run::<false>(&[])
            .expect("first invocation should succeed");
        assert_eq!(vm.r[0].as_int(), 42);

        vm.reset();
        vm.pc = 0;
        vm.run::<false>(&[])
            .expect("second invocation should succeed");
        assert_eq!(vm.r[0].as_int(), 42);
    }

    #[test]
    fn prepare_run_discards_frames_left_by_a_trap() {
        let mut vm = Vm::new(VmConfig::default());
        vm.bytecode = packed([
            Op::LoadI { dst: 0, value: 1 },
            Op::LoadI { dst: 1, value: 0 },
            Op::IDiv {
                dst: 0,
                lhs: 0,
                rhs: 1,
            },
            Op::Ret,
            Op::Halt,
        ]);

        vm.reset();
        vm.pc = 0;
        assert!(matches!(
            vm.run::<false>(&[]),
            Err(Anomaly::DivisionByZero { .. })
        ));

        vm.bytecode[2] = Op::LoadI { dst: 0, value: 7 }.encode();
        vm.reset();
        vm.pc = 0;
        vm.run::<false>(&[])
            .expect("invocation after a trap should succeed");
        assert_eq!(vm.r[0].as_int(), 7);
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
        assert_eq!(vm.r[2].as_int(), 42);
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
        assert_eq!(vm.r[2].as_int(), 42);
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
        assert_eq!(vm.r[2].as_int(), 42);
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
        assert_eq!(vm.r[2].as_int(), 42);
    }

    #[test]
    fn assert_traps_on_false_only() {
        let assert = Op::Assert {
            cond: 0,
            check: crate::Check::Assertion,
        };
        run(vec![Op::LoadI { dst: 0, value: 1 }, assert]);
        let err = run_err(vec![Op::LoadI { dst: 0, value: 0 }, assert]);
        assert!(matches!(err, Anomaly::AssertionFailed { pc: 1 }));
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
        assert_eq!(vm.r[2].as_int(), 3);
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
        assert_eq!(vm.r[1].as_int(), -3);
    }

    #[test]
    fn iand() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: 13 },
            Op::LoadI { dst: 1, value: 6 },
            Op::IAnd {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
        ]);
        assert_eq!(vm.r[2].as_int(), 4);
    }

    /// The immediate sign extends, like the jit's `and r64, imm32`.
    #[test]
    fn iand_i() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: -3 },
            Op::IAndI {
                dst: 1,
                lhs: 0,
                imm: -4,
            },
        ]);
        assert_eq!(vm.r[1].as_int(), -4);
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

    /// Table of 3 slots from `first`: slot 0 loads 10, slot 1 has no key and
    /// holds the miss pc, slot 2 loads 30, a miss loads -1.
    fn switch(kind: SwitchKind, first: Value, subject: Value) -> i64 {
        let mut vm = Vm::new(VmConfig::default());
        vm.switch_tables = Box::new([SwitchTable {
            first: first.0,
            targets: Box::new([4, 2, 6]),
        }]);
        vm.globals = Box::new([subject]);
        vm.bytecode = packed([
            Op::LoadG { dst: 0, idx: 0 },
            Op::Switch {
                kind,
                subject: 0,
                table: 0,
            },
            Op::LoadI { dst: 1, value: -1 },
            Op::Halt,
            Op::LoadI { dst: 1, value: 10 },
            Op::Halt,
            Op::LoadI { dst: 1, value: 30 },
            Op::Halt,
        ]);
        vm.run::<false>(&[]).unwrap();
        vm.r[1].as_int()
    }

    #[test]
    fn switch_indexes_strings_by_pool_offset() {
        let key = |offset: u64| Value(0x1000 + offset);
        let slot = string::ALIGN as u64;
        assert_eq!(switch(SwitchKind::Str, key(0), key(0)), 10);
        assert_eq!(switch(SwitchKind::Str, key(0), key(2 * slot)), 30);
        assert_eq!(switch(SwitchKind::Str, key(0), key(slot)), -1);
        assert_eq!(switch(SwitchKind::Str, key(0), key(3 * slot)), -1);
        assert_eq!(switch(SwitchKind::Str, key(slot), key(0)), -1);
    }

    #[test]
    fn lookup_takes_the_default_outside_its_keys() {
        let lookup = |subject: i64| {
            let mut vm = Vm::new(VmConfig::default());
            vm.lookup_tables = Box::new([LookupTable {
                first: Value::from(-1_i64).0,
                values: Box::new([
                    Value::from(10_i64),
                    Value::from(-1_i64),
                    Value::from(30_i64),
                ]),
                default: Value::from(-1_i64),
            }]);
            vm.globals = Box::new([Value::from(subject)]);
            vm.bytecode = packed([
                Op::LoadG { dst: 0, idx: 0 },
                Op::Lookup {
                    kind: SwitchKind::Int,
                    dst: 1,
                    subject: 0,
                    table: 0,
                },
                Op::Halt,
            ]);
            vm.run::<false>(&[]).unwrap();
            vm.r[1].as_int()
        };
        assert_eq!(lookup(-1), 10);
        assert_eq!(lookup(1), 30);
        assert_eq!(lookup(0), -1);
        assert_eq!(lookup(2), -1);
        assert_eq!(lookup(-2), -1);
    }

    #[test]
    fn switch_indexes_ints_from_the_smallest_key() {
        let int = |v: i64| Value::from(v);
        assert_eq!(switch(SwitchKind::Int, int(-1), int(-1)), 10);
        assert_eq!(switch(SwitchKind::Int, int(-1), int(1)), 30);
        assert_eq!(switch(SwitchKind::Int, int(-1), int(0)), -1);
        assert_eq!(switch(SwitchKind::Int, int(-1), int(2)), -1);
        assert_eq!(switch(SwitchKind::Int, int(-1), int(-2)), -1);
    }

    /// Unlike integer division, IEEE 754 defines it: no trap.
    #[test]
    fn ddiv_by_zero_is_inf() {
        let mut vm = Vm::new(VmConfig::default());
        vm.globals = Box::new([Value::from(4.0_f64), Value::from(-0.0_f64)]);
        vm.bytecode = packed([
            Op::LoadG { dst: 0, idx: 0 },
            Op::LoadG { dst: 1, idx: 1 },
            Op::DDiv {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
            Op::Halt,
        ]);
        vm.run::<false>(&[]).unwrap();
        assert_eq!(vm.r[2].as_f64(), f64::NEG_INFINITY);
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
        assert!(!vm.r[2].as_bool());
        assert!(vm.r[3].as_bool());
        assert!(!vm.r[4].as_bool());
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
        assert!(vm.r[1].as_bool());
        assert!(!vm.r[2].as_bool());
    }

    #[test]
    fn double_arith() {
        let mut vm = Vm::new(VmConfig::default());
        vm.globals = Box::new([Value::from(1.5_f64), Value::from(2.5_f64)]);
        vm.bytecode = packed([
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
            Op::Halt,
        ]);
        vm.run::<false>(&[]).unwrap();
        assert_eq!(vm.r[2].as_f64(), 4.0);
        assert_eq!(vm.r[3].as_f64(), 1.0);
        assert_eq!(vm.r[4].as_f64(), 3.75);
        assert_eq!(vm.r[5].as_f64(), 2.5 / 1.5);
    }

    #[test]
    fn mov() {
        let vm = run(vec![
            Op::LoadI { dst: 5, value: 99 },
            Op::Mov { dst: 0, src: 5 },
        ]);
        assert_eq!(vm.r[0].as_int(), 99);
        assert_eq!(vm.r[5].as_int(), 99);
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

        let payload = vm.r[0].as_ptr::<u8>();
        // SAFETY: Alloc reserved 16 bytes, offset 8 is in bounds
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

        assert_eq!(vm.r[2].as_int(), 42);
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

        assert_eq!(vm.r[3].as_int(), 42);
    }

    #[test]
    fn jmp_skips_instructions() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: 1 },
            Op::Jmp { target: 3 },
            Op::LoadI { dst: 0, value: 999 }, // skipped
            Op::LoadI { dst: 1, value: 2 },
        ]);
        assert_eq!(vm.r[0].as_int(), 1);
        assert_eq!(vm.r[1].as_int(), 2);
    }

    #[test]
    fn jmpt_jumps_when_cond_is_true() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: 1 }, // truthy
            Op::JmpT { cond: 0, target: 3 },
            Op::LoadI { dst: 1, value: 999 }, // skipped
            Op::LoadI { dst: 2, value: 7 },
        ]);
        assert_eq!(vm.r[1].as_int(), 0);
        assert_eq!(vm.r[2].as_int(), 7);
    }

    #[test]
    fn jmpt_falls_through_when_cond_is_false() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: 0 },
            Op::JmpT { cond: 0, target: 3 },
            Op::LoadI { dst: 1, value: 11 },
            Op::LoadI { dst: 2, value: 22 },
        ]);
        assert_eq!(vm.r[1].as_int(), 11);
        assert_eq!(vm.r[2].as_int(), 22);
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
        assert_eq!(vm.r[0].as_int(), 10);
        assert_eq!(vm.r[1].as_int(), 20);
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
        assert_eq!(vm.r[0].as_int(), 10);
        assert_eq!(vm.r[1].as_int(), 20);
        assert_eq!(vm.r[2].as_int(), 30);
        assert_eq!(vm.r[3].as_int(), 40);
        assert_eq!(vm.r[4].as_int(), 50);
    }

    /// A slot counts from the top of the spill stack: a push on top of it moves it one further down.
    #[test]
    fn slots_count_from_the_top() {
        let vm = run(vec![
            Op::LoadI { dst: 0, value: 10 },
            Op::LoadI { dst: 1, value: 20 },
            Op::Push2 { a: 0, b: 0 },
            Op::StoreSlot { slot: 1, src: 1 },
            Op::StoreSlot { slot: 0, src: 0 },
            Op::Push { src: 0 },
            Op::LoadSlot { dst: 2, slot: 2 },
            Op::LoadSlot { dst: 3, slot: 1 },
            Op::Pop3 { a: 4, b: 4, c: 4 },
        ]);
        assert_eq!(vm.r[2].as_int(), 20);
        assert_eq!(vm.r[3].as_int(), 10);
    }

    #[test]
    fn casts() {
        let mut vm = Vm::new(VmConfig::default());
        vm.globals = Box::new([Value::from(3.7_f64)]);
        vm.bytecode = packed([
            Op::LoadI { dst: 0, value: 5 },
            Op::CastToDouble { dst: 1, src: 0 },
            Op::LoadG { dst: 2, idx: 0 },
            Op::CastToInt { dst: 3, src: 2 },
            Op::CastToBool { dst: 4, src: 0 },
            Op::LoadI { dst: 5, value: 0 },
            Op::CastToBool { dst: 6, src: 5 },
            Op::Halt,
        ]);
        vm.run::<false>(&[]).unwrap();
        assert_eq!(vm.r[1].as_f64(), 5.0);
        assert_eq!(vm.r[3].as_int(), 3);
        assert!(vm.r[4].as_bool());
        assert!(!vm.r[6].as_bool());
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
        assert_eq!(vm.r[1].as_int(), 7);
        assert_eq!(vm.r[2].as_int(), 99);
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
        assert!(vm.r[3].as_bool());
        assert!(!vm.r[4].as_bool());
    }
}
