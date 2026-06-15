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

fn print_function_head(fun: &Fn<'_>, f: &mut dyn fmt::Write) -> fmt::Result {
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

/// `|`-join type names, deduped and order-preserving: `Str|Int|Double`, or just
/// `Str` when every variant agrees.
fn type_union(types: impl Iterator<Item = String>) -> String {
    let mut seen: Vec<String> = Vec::new();
    for t in types {
        if !seen.contains(&t) {
            seen.push(t);
        }
    }
    seen.join("|")
}

/// Comma-joined arg types of one specialisation, e.g. `Int` or `Str, Int`.
fn variant_arg_sig(fun: &Fn<'_>) -> String {
    fun.args
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Render one overload group. A single-fn group prints as a normal signature;
/// a specialisation group merges into a unioned head plus a per-variant doc line:
///
/// ```text
/// fn from(Int|Double) Str
///    Int -> ...
///    Double -> ...
/// ```
pub fn print_overload_group(
    name: &str,
    variants: &[&Fn<'_>],
    f: &mut dyn fmt::Write,
) -> fmt::Result {
    if let [single] = variants {
        return print_function_head(single, f);
    }

    write!(f, "fn {name}(")?;
    let argc = variants[0].args.len();
    for pos in 0..argc {
        write!(f, "{}", type_union(variants.iter().map(|v| v.args[pos].to_string())))?;
        if pos + 1 < argc {
            write!(f, " ")?;
        }
    }
    // ret can vary across variants (e.g. debug's `T -> T`), so union it too.
    writeln!(f, ") {}", type_union(variants.iter().map(|v| v.ret.to_string())))?;

    for v in variants {
        writeln!(f, "   {} -> {}", variant_arg_sig(v), v.doc)?;
    }
    Ok(())
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
            for (name, variants) in self.overload_groups() {
                print_overload_group(name, &variants, f)?;
            }
        }

        Ok(())
    }
}

impl Pkg {
    /// Group `fns` by public name (a `specialises` group, or the fn's own name),
    /// preserving first-seen order. Single source of truth for overload-aware
    /// doc rendering and lookup.
    #[must_use]
    pub fn overload_groups(&self) -> Vec<(&'static str, Vec<&Fn<'static>>)> {
        let mut groups: Vec<(&'static str, Vec<&Fn<'static>>)> = Vec::new();
        for fun in self.fns {
            let key = fun.group_name();
            if let Some(group) = groups.iter_mut().find(|(k, _)| *k == key) {
                group.1.push(fun);
            } else {
                groups.push((key, vec![fun]));
            }
        }
        groups
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
