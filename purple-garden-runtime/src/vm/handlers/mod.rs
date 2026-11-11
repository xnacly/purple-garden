//! Threaded dispatch: each handler ends by tail calling the next op's handler, so every handler
//! owns its indirect jump. Handlers live in the module named after the [`Packed`] class they
//! decode.

use super::*;

type V = *mut Vm<'static>;

type Handler = unsafe fn(V, *const Packed, usize, Packed) -> usize;

struct Handlers<const B: bool>;

impl<const B: bool> Handlers<B> {
    const TABLE: [Handler; 64] = {
        let mut table: [Handler; 64] = [halt::<B>; 64];
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
            Tag::IAdd => rrr::i_add::<B>,
            Tag::IAddI => rri16::i_add_i::<B>,
            Tag::ISub => rrr::i_sub::<B>,
            Tag::ISubI => rri16::i_sub_i::<B>,
            Tag::IMul => rrr::i_mul::<B>,
            Tag::IMulI => rri16::i_mul_i::<B>,
            Tag::IDiv => rrr::i_div::<B>,
            Tag::IDivI => rri16::i_div_i::<B>,
            Tag::IMod => rrr::i_mod::<B>,
            Tag::IModI => rri16::i_mod_i::<B>,
            Tag::IAnd => rrr::i_and::<B>,
            Tag::IAndI => rri16::i_and_i::<B>,
            Tag::ILt => rrr::i_lt::<B>,
            Tag::IGt => rrr::i_gt::<B>,
            Tag::IEq => rrr::i_eq::<B>,
            Tag::IEqI => rri16::i_eq_i::<B>,
            Tag::IGtI => rri16::i_gt_i::<B>,
            Tag::ILtI => rri16::i_lt_i::<B>,
            Tag::DAdd => rrr::d_add::<B>,
            Tag::DSub => rrr::d_sub::<B>,
            Tag::DMul => rrr::d_mul::<B>,
            Tag::DDiv => rrr::d_div::<B>,
            Tag::DLt => rrr::d_lt::<B>,
            Tag::DGt => rrr::d_gt::<B>,
            Tag::BEq => rrr::b_eq::<B>,
            Tag::SEq => rrr::s_eq::<B>,
            Tag::Mov => rr::mov::<B>,
            Tag::LoadI => ri21::load_i::<B>,
            Tag::LoadG => ru21::load_g::<B>,
            Tag::Jmp => t16::jmp::<B>,
            Tag::JmpT => rt16::jmp_t::<B>,
            Tag::JmpF => rt16::jmp_f::<B>,
            Tag::JmpEqI => ri5t16::jmp_eq_i::<B>,
            Tag::JmpNeI => ri5t16::jmp_ne_i::<B>,
            Tag::JmpSEq => rrt16::jmp_s_eq::<B>,
            Tag::JmpSNe => rrt16::jmp_s_ne::<B>,
            Tag::Switch => rk1u20::switch::<B>,
            Tag::Lookup => rrk1u15::lookup::<B>,
            Tag::Tail => u26::tail::<B>,
            Tag::Call => u26::call::<B>,
            Tag::Sys => u5u16::sys::<B>,
            Tag::Native => u16k1::native::<B>,
            Tag::Push => r::push::<B>,
            Tag::Push2 => rr::push2::<B>,
            Tag::Push3 => rrr::push3::<B>,
            Tag::Pop => r::pop::<B>,
            Tag::Pop2 => rr::pop2::<B>,
            Tag::Pop3 => rrr::pop3::<B>,
            Tag::StoreSlot => ru21::store_slot::<B>,
            Tag::LoadSlot => ru21::load_slot::<B>,
            Tag::CastToInt => rr::cast_to_int::<B>,
            Tag::CastToDouble => rr::cast_to_double::<B>,
            Tag::CastToBool => rr::cast_to_bool::<B>,
            Tag::Alloc => rk2u3u16::alloc::<B>,
            Tag::Store => rru16::store::<B>,
            Tag::Load => rru16::load::<B>,
            Tag::AddrOf => rru16::addr_of::<B>,
            Tag::Ret => ret::<B>,
            Tag::Assert => r::assert::<B>,
            Tag::Halt => halt::<B>,
            Tag::Nop => nop::<B>,
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

mod r;
mod ri21;
mod ri5t16;
mod rk1u20;
mod rk2u3u16;
mod rr;
mod rri16;
mod rrk1u15;
mod rrr;
mod rrt16;
mod rru16;
mod rt16;
mod ru21;
mod t16;
mod u16k1;
mod u26;
mod u5u16;

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

unsafe fn ret<const B: bool>(vm: V, code: *const Packed, _pc: usize, _op: Packed) -> usize {
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

unsafe fn halt<const B: bool>(_vm: V, _code: *const Packed, pc: usize, _op: Packed) -> usize {
    pc
}

unsafe fn nop<const B: bool>(vm: V, code: *const Packed, pc: usize, _op: Packed) -> usize {
    unsafe { next!(vm, code, pc.wrapping_add(1)) }
}
