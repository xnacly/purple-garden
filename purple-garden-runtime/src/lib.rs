#![feature(likely_unlikely)]

use std::fmt::{self, Write as _};

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
    pub args: &'static [Type<'static>],
    pub ret: Type<'static>,
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

impl Pkg {
    /// Render this package as `extern.garden` source.
    ///
    /// The generated text is meant for tooling and editor integration. It
    /// carries package and function signatures, argument names, docs, and the
    /// package hierarchy, but it does not include VM wrapper pointers.
    ///
    /// Typical consumers write the output to an `extern.garden` file and feed
    /// that into analysis tooling such as an LSP or package signature index.
    #[must_use]
    pub fn extern_source(&self) -> String {
        let mut out = String::new();
        fmt_extern_pkg(self, 0, &mut out).unwrap();
        out
    }
}

fn fmt_extern_pkg(pkg: &Pkg, indent: usize, out: &mut String) -> fmt::Result {
    let pad = "    ".repeat(indent);
    if !pkg.doc.is_empty() {
        for line in pkg.doc.lines() {
            if line.is_empty() {
                writeln!(out, "{pad}#!")?;
            } else {
                writeln!(out, "{pad}#! {line}")?;
            }
        }
    }
    writeln!(out, "{pad}extern \"{}\" {{", pkg.name)?;

    for fun in pkg.fns {
        if !fun.doc.is_empty() {
            for line in fun.doc.lines() {
                if line.is_empty() {
                    writeln!(out, "{pad}    #!")?;
                } else {
                    writeln!(out, "{pad}    #! {line}")?;
                }
            }
        }
        write!(out, "{pad}    fn {}(", fun.name)?;
        for (i, (arg_name, arg_ty)) in fun.arg_names.iter().zip(fun.args.iter()).enumerate() {
            write!(out, "{arg_name}: {arg_ty}")?;
            if i + 1 != fun.args.len() {
                write!(out, ", ")?;
            }
        }
        write!(out, ")")?;
        if fun.ret != Type::Void {
            write!(out, " {}", fun.ret)?;
        }
        writeln!(out)?;
    }

    for sub in pkg.pkgs {
        fmt_extern_pkg(sub, indent + 1, out)?;
    }

    writeln!(out, "{pad}}}")
}
