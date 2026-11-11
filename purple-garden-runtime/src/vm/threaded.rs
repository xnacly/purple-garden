//! Threaded dispatch: each handler ends by tail calling the next op's handler, so every handler
//! owns its indirect jump.
#![allow(non_snake_case)]

use super::*;

type V = *mut Vm<'static>;

type Handler = unsafe fn(V, *const Packed, usize, Packed) -> usize;

struct Handlers<const B: bool>;

impl<const B: bool> Handlers<B> {
    const TABLE: [Handler; 64] = {
        let mut table: [Handler; 64] = [h_Halt::<B>; 64];
        let mut i = 0;
        while i < Tag::COUNT {
            // SAFETY: Tag is repr(u8) with discriminants 0..COUNT
            table[i] = Self::handler(unsafe { std::mem::transmute::<u8, Tag>(i as u8) });
            i += 1;
        }
        table
    };

    const fn handler(tag: Tag) -> Handler {
        match tag {
            Tag::IAdd => h_IAdd::<B>,
            Tag::IAddI => h_IAddI::<B>,
            Tag::ISub => h_ISub::<B>,
            Tag::ISubI => h_ISubI::<B>,
            Tag::IMul => h_IMul::<B>,
            Tag::IMulI => h_IMulI::<B>,
            Tag::IDiv => h_IDiv::<B>,
            Tag::IDivI => h_IDivI::<B>,
            Tag::IMod => h_IMod::<B>,
            Tag::IModI => h_IModI::<B>,
            Tag::IAnd => h_IAnd::<B>,
            Tag::IAndI => h_IAndI::<B>,
            Tag::ILt => h_ILt::<B>,
            Tag::IGt => h_IGt::<B>,
            Tag::IEq => h_IEq::<B>,
            Tag::IEqI => h_IEqI::<B>,
            Tag::IGtI => h_IGtI::<B>,
            Tag::ILtI => h_ILtI::<B>,
            Tag::DAdd => h_DAdd::<B>,
            Tag::DSub => h_DSub::<B>,
            Tag::DMul => h_DMul::<B>,
            Tag::DDiv => h_DDiv::<B>,
            Tag::DLt => h_DLt::<B>,
            Tag::DGt => h_DGt::<B>,
            Tag::BEq => h_BEq::<B>,
            Tag::SEq => h_SEq::<B>,
            Tag::Mov => h_Mov::<B>,
            Tag::LoadI => h_LoadI::<B>,
            Tag::LoadG => h_LoadG::<B>,
            Tag::Jmp => h_Jmp::<B>,
            Tag::JmpT => h_JmpT::<B>,
            Tag::JmpF => h_JmpF::<B>,
            Tag::JmpEqI => h_JmpEqI::<B>,
            Tag::JmpNeI => h_JmpNeI::<B>,
            Tag::JmpSEq => h_JmpSEq::<B>,
            Tag::JmpSNe => h_JmpSNe::<B>,
            Tag::Switch => h_Switch::<B>,
            Tag::Lookup => h_Lookup::<B>,
            Tag::Tail => h_Tail::<B>,
            Tag::Call => h_Call::<B>,
            Tag::Sys => h_Sys::<B>,
            Tag::Native => h_Native::<B>,
            Tag::Push => h_Push::<B>,
            Tag::Push2 => h_Push2::<B>,
            Tag::Push3 => h_Push3::<B>,
            Tag::Pop => h_Pop::<B>,
            Tag::Pop2 => h_Pop2::<B>,
            Tag::Pop3 => h_Pop3::<B>,
            Tag::StoreSlot => h_StoreSlot::<B>,
            Tag::LoadSlot => h_LoadSlot::<B>,
            Tag::CastToInt => h_CastToInt::<B>,
            Tag::CastToDouble => h_CastToDouble::<B>,
            Tag::CastToBool => h_CastToBool::<B>,
            Tag::Alloc => h_Alloc::<B>,
            Tag::Store => h_Store::<B>,
            Tag::Load => h_Load::<B>,
            Tag::AddrOf => h_AddrOf::<B>,
            Tag::Ret => h_Ret::<B>,
            Tag::Assert => h_Assert::<B>,
            Tag::Halt => h_Halt::<B>,
            Tag::Nop => h_Nop::<B>,
        }
    }
}

// Registers sit at offset 0 of the vm, so the vm pointer doubles as theirs.
macro_rules! r {
    ($vm:ident, $n:tt) => {
        (&*($vm as V).cast::<Value>().add($n as usize))
    };
}

macro_rules! r_mut {
    ($vm:ident, $n:tt) => {
        *($vm as V).cast::<Value>().add($n as usize)
    };
}

macro_rules! next {
    ($vm:ident, $code:ident, $pc:expr) => {{
        let pc = $pc;
        let op = *$code.add(pc);
        become (Handlers::<B>::TABLE[op.tag() as usize])($vm as V, $code, pc, op)
    }};
}

/// Parks the anomaly in [`Vm::pending_trap`] for [`enter`] to return, so handlers return a
/// register sized pc instead of a `Result` through memory.
macro_rules! trap {
    ($vm:ident, $anomaly:expr) => {{
        (*($vm as V)).pending_trap = Some($anomaly);
        return 0;
    }};
}

macro_rules! trap_if {
    ($vm:ident, $condition:expr, $anomaly:expr) => {
        if std::hint::unlikely($condition) {
            trap!($vm, $anomaly);
        }
    };
}

pub(super) unsafe fn enter<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
) -> Result<usize, Anomaly> {
    unsafe {
        let op = *code.add(pc);
        let pc = (Handlers::<B>::TABLE[op.tag() as usize])(vm, code, pc, op);
        (*vm).pending_trap.take().map_or(Ok(pc), Err)
    }
}

unsafe fn h_LoadI<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, value) = op.ri21();
        r_mut!(vm, dst) = Value::from(value as i64);
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: bc only emits in-bounds registers and globals
unsafe fn h_LoadG<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, idx) = op.ru21();
        r_mut!(vm, dst) = *(*vm).globals.as_ptr().add(idx as usize);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_IAdd<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        r_mut!(vm, dst) = Value::from(l + r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_IAddI<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l + imm as i64);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_ISub<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        r_mut!(vm, dst) = Value::from(l - r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_ISubI<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l - imm as i64);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_IMul<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        r_mut!(vm, dst) = Value::from(l * r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_IMulI<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l * imm as i64);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_IDiv<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        trap_if!(vm, r == 0, Anomaly::DivisionByZero { pc });
        r_mut!(vm, dst) = Value::from(l / r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_IDivI<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let imm = imm as i64;
        trap_if!(vm, imm == 0, Anomaly::DivisionByZero { pc });
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l / imm);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_IMod<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        trap_if!(vm, r == 0, Anomaly::DivisionByZero { pc });
        r_mut!(vm, dst) = Value::from(l % r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_IModI<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let imm = imm as i64;
        trap_if!(vm, imm == 0, Anomaly::DivisionByZero { pc });
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l % imm);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_IAnd<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        r_mut!(vm, dst) = Value::from(l & r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_IAndI<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l & imm as i64);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_IEq<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        r_mut!(vm, dst) = Value::from(l == r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_IEqI<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l == imm as i64);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_IGt<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        r_mut!(vm, dst) = Value::from(l > r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_IGtI<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l > imm as i64);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_ILt<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_int();
        let r = r!(vm, rhs).as_int();
        r_mut!(vm, dst) = Value::from(l < r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_ILtI<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, imm) = op.rri16();
        let l = r!(vm, lhs).as_int();
        r_mut!(vm, dst) = Value::from(l < imm as i64);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_DAdd<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_f64();
        let r = r!(vm, rhs).as_f64();
        r_mut!(vm, dst) = Value::from(l + r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_DSub<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_f64();
        let r = r!(vm, rhs).as_f64();
        r_mut!(vm, dst) = Value::from(l - r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_DMul<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_f64();
        let r = r!(vm, rhs).as_f64();
        r_mut!(vm, dst) = Value::from(l * r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_DDiv<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_f64();
        let r = r!(vm, rhs).as_f64();
        r_mut!(vm, dst) = Value::from(l / r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_DGt<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_f64();
        let r = r!(vm, rhs).as_f64();
        r_mut!(vm, dst) = Value::from(l > r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_DLt<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_f64();
        let r = r!(vm, rhs).as_f64();
        r_mut!(vm, dst) = Value::from(l < r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_BEq<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        let l = r!(vm, lhs).as_bool();
        let r = r!(vm, rhs).as_bool();
        r_mut!(vm, dst) = Value::from(l == r);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_SEq<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, lhs, rhs) = op.rrr();
        r_mut!(vm, dst) = Value::from(r!(vm, lhs).str_eq(r!(vm, rhs)));
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_Mov<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, src) = op.rr();
        r_mut!(vm, dst) = *r!(vm, src);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_Jmp<const B: bool>(vm: V, code: *const Packed, _pc: usize, op: Packed) -> usize {
    unsafe {
        let target = op.t16();
        next!(vm, code, target as usize)
    }
}

unsafe fn h_Tail<const B: bool>(vm: V, code: *const Packed, _pc: usize, op: Packed) -> usize {
    unsafe {
        let vm = &mut *vm;
        let func = op.u26();
        if B {
            vm.backtrace.push(func as usize);
        }
        next!(vm, code, func as usize)
    }
}

unsafe fn h_JmpT<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (cond, target) = op.rt16();
        if r!(vm, cond).as_bool() {
            next!(vm, code, target as usize);
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_JmpF<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (cond, target) = op.rt16();
        if !r!(vm, cond).as_bool() {
            next!(vm, code, target as usize);
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_Assert<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let cond = op.r();
        trap_if!(vm, !r!(vm, cond).as_bool(), Anomaly::AssertionFailed { pc });
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_JmpEqI<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (lhs, imm, target) = op.ri5t16();
        if r!(vm, lhs).as_int() == imm as i64 {
            next!(vm, code, target as usize);
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_JmpNeI<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (lhs, imm, target) = op.ri5t16();
        if r!(vm, lhs).as_int() != imm as i64 {
            next!(vm, code, target as usize);
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_JmpSEq<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (lhs, rhs, target) = op.rrt16();
        if r!(vm, lhs).str_eq(r!(vm, rhs)) {
            next!(vm, code, target as usize);
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_JmpSNe<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (lhs, rhs, target) = op.rrt16();
        if !r!(vm, lhs).str_eq(r!(vm, rhs)) {
            next!(vm, code, target as usize);
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: bc only emits indices into vm.switch_tables
unsafe fn h_Switch<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (subject, kind, table) = op.rk1u20();
        let table = &*(*vm).switch_tables.as_ptr().add(table as usize);
        // Strings are interned, equal strings share one address. string key is its
        // address in the string pool, an int key its value. Wrapping keeps a subject
        // below `first` huge, so the bounds check below rejects it along with those
        // past the end.
        let offset = r!(vm, subject).0.wrapping_sub(table.first);
        // Pool strings sit ALIGN bytes apart, one slot each.
        let slot = match kind {
            SwitchKind::Str => offset / string::ALIGN as u64,
            SwitchKind::Int => offset,
        };
        // Slots between keys hold the default's pc; outside the
        // table, fall through to the `jmp default` after this op.
        if let Some(&target) = table.targets.get(slot as usize) {
            next!(vm, code, target as usize);
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: bc only emits indices into vm.lookup_tables
unsafe fn h_Lookup<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, subject, kind, table) = op.rrk1u15();
        let table = &*(*vm).lookup_tables.as_ptr().add(table as usize);
        let offset = r!(vm, subject).0.wrapping_sub(table.first);
        let slot = match kind {
            SwitchKind::Str => offset / string::ALIGN as u64,
            SwitchKind::Int => offset,
        };
        r_mut!(vm, dst) = *table.values.get(slot as usize).unwrap_or(&table.default);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_Call<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let vm = &mut *vm;
        let func = op.u26();
        if B {
            vm.backtrace.push(func as usize);
        }

        let depth = vm.frame_depth;
        if std::hint::unlikely(depth == vm.frames.len()) {
            trap!(vm, Anomaly::StackOverflow { pc });
        }

        // SAFETY: depth < frames.len() checked above
        *vm.frames.get_unchecked_mut(depth) = CallFrame {
            return_to: pc,
            #[cfg(debug_assertions)]
            spilled_depth: (*vm.spilled).depth(),
        };
        vm.frame_depth = depth + 1;

        next!(vm, code, func as usize)
    }
}

// SAFETY: bc only emits Sys indices present in syscalls
unsafe fn h_Sys<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let vm = &mut *vm;
        let (argc, idx) = op.u5u16();
        #[cfg(not(debug_assertions))]
        let _ = argc;
        #[cfg(debug_assertions)]
        let pre_sys: [Value; REGISTER_COUNT] = vm.r;

        // Publish the current pc before the call: a syscall that
        // traps reads vm.pc to locate itself, but the loop only
        // writes vm.pc on exit, so without this the trap would
        // carry a stale pc and render at the wrong source span.
        vm.pc = pc;
        (*vm.syscalls.add(idx as usize))((vm as V).cast());

        #[cfg(debug_assertions)]
        for (i, pre) in pre_sys.iter().enumerate().skip(usize::from(argc.max(1))) {
            debug_assert_eq!(
                pre.0,
                vm.r[i].0,
                "syscall idx={idx} wrote r{i}; convention only permits writes to r0..r{}",
                argc.max(1) - 1
            );
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: idx present in syscalls, vm not otherwise borrowed
unsafe fn h_Native<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let vm = &mut *vm;
        let (idx, returns_double) = op.u16k1();
        vm.pc = pc;
        let enter = if returns_double {
            crate::enter_native_double
        } else {
            crate::enter_native
        };
        enter((vm as V).cast(), *vm.syscalls.add(idx as usize));
        if std::hint::unlikely(vm.pending_trap.is_some()) {
            return 0;
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_Ret<const B: bool>(vm: V, code: *const Packed, _pc: usize, _op: Packed) -> usize {
    unsafe {
        let vm = &mut *vm;
        if B {
            vm.backtrace.pop();
        }

        // The root frame from Vm::new keeps this from underflowing.
        let depth = vm.frame_depth - 1;
        vm.frame_depth = depth;
        let frame = *vm.frames.get_unchecked(depth);

        // See Op::Push: every function must leave the spill
        // stack at the depth it found it. Catches arg-shuffle
        // cycle paths or caller-save spills that forgot to
        // pair their Pops.
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            frame.spilled_depth,
            (*vm.spilled).depth(),
            "function returning to pc={} left the spill stack unbalanced (entered at depth {}, exiting at depth {})",
            frame.return_to,
            frame.spilled_depth,
            (*vm.spilled).depth(),
        );
        next!(vm, code, frame.return_to.wrapping_add(1))
    }
}

unsafe fn h_Push<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let src = op.r();
        if std::hint::unlikely(!(*(*vm).spilled).push(*r!(vm, src))) {
            trap!(vm, Anomaly::StackOverflow { pc });
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_Push2<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (a, b) = op.rr();
        if std::hint::unlikely(!(*(*vm).spilled).push_all([*r!(vm, a), *r!(vm, b)])) {
            trap!(vm, Anomaly::StackOverflow { pc });
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_Push3<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (a, b, c) = op.rrr();
        if std::hint::unlikely(!(*(*vm).spilled).push_all([*r!(vm, a), *r!(vm, b), *r!(vm, c)])) {
            trap!(vm, Anomaly::StackOverflow { pc });
        }
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: bc pairs every Pop with an earlier Push
unsafe fn h_Pop<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let dst = op.r();
        debug_assert!((*(*vm).spilled).depth() >= size_of::<Value>());
        r_mut!(vm, dst) = (*(*vm).spilled).pop::<Value>();
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: bc pairs every Pop with an earlier Push
unsafe fn h_Pop2<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (a, b) = op.rr();
        debug_assert!((*(*vm).spilled).depth() >= 2 * size_of::<Value>());
        let [below, top] = (*(*vm).spilled).pop_all::<Value, 2>();
        r_mut!(vm, a) = top;
        r_mut!(vm, b) = below;
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: bc pairs every Pop with an earlier Push
unsafe fn h_Pop3<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (a, b, c) = op.rrr();
        debug_assert!((*(*vm).spilled).depth() >= 3 * size_of::<Value>());
        let [bottom, below, top] = (*(*vm).spilled).pop_all::<Value, 3>();
        r_mut!(vm, a) = top;
        r_mut!(vm, b) = below;
        r_mut!(vm, c) = bottom;
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: bc only emits registers below REGISTER_COUNT, and slots inside the frame it
// reserved on the spill stack
unsafe fn h_StoreSlot<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (src, slot) = op.ru21();
        (*(*vm).spilled).poke(slot as usize, *r!(vm, src));
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: see StoreSlot
unsafe fn h_LoadSlot<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, slot) = op.ru21();
        r_mut!(vm, dst) = (*(*vm).spilled).peek::<Value>(slot as usize);
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_CastToDouble<const B: bool>(
    vm: V,
    code: *const Packed,
    pc: usize,
    op: Packed,
) -> usize {
    unsafe {
        let (dst, src) = op.rr();
        r_mut!(vm, dst) = r!(vm, src).int_to_f64();
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_CastToInt<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, src) = op.rr();
        r_mut!(vm, dst) = r!(vm, src).f64_to_int();
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_CastToBool<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, src) = op.rr();
        r_mut!(vm, dst) = r!(vm, src).int_to_bool();
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: size and align come from a Layout in lowering
unsafe fn h_Alloc<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let vm = &mut *vm;
        let (dst, kind, align, size) = op.rk2u3u16();
        let ptr = vm.alloc(
            kind,
            Layout::from_size_align_unchecked(size as usize, align as usize),
        );
        r_mut!(vm, dst) = Value::from_ptr(ptr.as_ptr());
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: offset lies within the block base was allocated as
unsafe fn h_Store<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (src, base, offset) = op.rru16();
        (r!(vm, base).as_ptr::<u8>().add(offset as usize) as *mut Value).write(*r!(vm, src));
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: offset lies within the block base was allocated as
unsafe fn h_Load<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, base, offset) = op.rru16();
        r_mut!(vm, dst) = *(r!(vm, base).as_ptr::<u8>().add(offset as usize) as *const Value);
        next!(vm, code, pc.wrapping_add(1))
    }
}

// SAFETY: offset lies within the block base was allocated as
unsafe fn h_AddrOf<const B: bool>(vm: V, code: *const Packed, pc: usize, op: Packed) -> usize {
    unsafe {
        let (dst, base, offset) = op.rru16();
        r_mut!(vm, dst) = Value::from_ptr(r!(vm, base).as_ptr::<u8>().add(offset as usize));
        next!(vm, code, pc.wrapping_add(1))
    }
}

unsafe fn h_Halt<const B: bool>(_vm: V, _code: *const Packed, pc: usize, _op: Packed) -> usize {
    pc
}

unsafe fn h_Nop<const B: bool>(vm: V, code: *const Packed, pc: usize, _op: Packed) -> usize {
    unsafe { next!(vm, code, pc.wrapping_add(1)) }
}
