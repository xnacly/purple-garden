#![feature(likely_unlikely)]

use std::fmt;

pub use purple_garden_ir::ptype::Type;

pub mod anomaly;
/// purple garden bytecode virtual machine operations
pub mod op;
pub mod value;
pub mod vm;

pub const REGISTER_COUNT: usize = 64;

pub use crate::anomaly::Anomaly;
pub use crate::value::{FromVm, IntoVm, PgType, Value};
pub use crate::vm::{jit_trap_div_zero, syscall_unimplemented, CallFrame, DebugInfo, Vm, VmConfig};

/// Signature for a purple garden syscall
///
/// Calling convention:
/// - Args are passed in `r0..r{argcount-1}`. Read them via `vm.r(i)` starting at 0.
/// - `r0` is also the return-value slot. Write the result via `*vm.r_mut(0) = value`.
///   Void functions leave `r0` untouched.
/// - Do not modify any register above r{argcount-1}. The bytecode emitter only spills
///   caller-save values in `r0..r{argcount-1}`, relying on this convention to leave
///   `r{argcount}+` untouched. A violation silently corrupts live values in release;
///   debug builds catch it via the `debug_assert_eq!` in [`Vm::run`]'s `Op::Sys` arm.
/// - Signal errors via [`Vm::trap`]; traps are checked at the next [`Op::Ret`].
pub type BuiltinFn = unsafe extern "C" fn(*mut Vm);

#[derive(Debug)]
pub struct Pkg {
    pub name: &'static str,
    pub doc: &'static str,
    pub pkgs: &'static [Pkg],
    pub fns: &'static [Fn],
}

#[derive(Debug)]
pub struct Fn {
    pub name: &'static str,
    pub doc: &'static str,
    pub ptr: BuiltinFn,
    pub pure: bool,
    pub arg_names: &'static [&'static str],
    pub args: &'static [Type],
    pub ret: Type,
}

fn print_function_head(fun: &Fn, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "fn {}(", fun.name)?;
    for (i, a) in fun.args.iter().enumerate() {
        if let Some(name) = fun.arg_names.get(i) {
            write!(f, "{name} ")?;
        }
        if i + 1 < fun.args.len() {
            write!(f, "{a} ")?;
        } else {
            write!(f, "{a}")?;
        }
    }
    writeln!(f, ") {}", fun.ret)
}

impl fmt::Display for Fn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        print_function_head(self, f)?;
        writeln!(f, "\t{}", self.doc)
    }
}

impl fmt::Display for Pkg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "import (\"{}\")\n", self.name)?;
        writeln!(f, "{}", self.doc)?;

        if !self.pkgs.is_empty() {
            writeln!(f)?;
            for p in self.pkgs {
                writeln!(f, "{}/{}", self.name, p.name)?;
            }
        }

        if !self.fns.is_empty() {
            writeln!(f)?;
            for fun in self.fns {
                print_function_head(fun, f)?;
            }
        }

        Ok(())
    }
}
