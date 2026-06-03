#![feature(likely_unlikely)]

use std::fmt::{self, Write as _};

pub use purple_garden_ir::{Fn, ptype::Type};
pub use purple_garden_shared::BuiltinFn;

pub mod anomaly;
/// purple garden bytecode virtual machine operations
pub mod op;
pub mod value;
pub mod vm;

pub const REGISTER_COUNT: usize = 64;

pub use crate::anomaly::Anomaly;
pub use crate::value::{FromVm, IntoVm, PgType, Value};
pub use crate::vm::{CallFrame, DebugInfo, Vm, VmConfig, jit_trap_div_zero, syscall_unimplemented};

#[derive(Debug)]
pub struct Pkg {
    pub name: &'static str,
    pub doc: &'static str,
    pub pkgs: &'static [Pkg],
    pub fns: &'static [Fn<'static>],
}

fn print_function_head(fun: &Fn<'_>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
