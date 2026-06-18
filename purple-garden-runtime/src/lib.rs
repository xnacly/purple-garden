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

/// Comma-joined arg types of one specialisation, e.g. `Int` or `Str, Int`.
fn variant_arg_sig(fun: &Fn<'_>) -> String {
    fun.args
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn variant_sig(fun: &Fn<'_>) -> String {
    format!("({}) -> {}", variant_arg_sig(fun), fun.ret)
}

/// Render one overload group. A single-fn group prints as a normal signature;
/// a specialisation group prints the group name plus aligned per-variant docs:
///
/// ```text
/// fn from:
///   (Int) -> Str     ...
///   (Double) -> Str  ...
/// ```
pub fn print_overload_group(
    name: &str,
    variants: &[&Fn<'_>],
    f: &mut dyn fmt::Write,
) -> fmt::Result {
    if let [single] = variants {
        print_function_head(single, f)?;
        if !single.doc.is_empty() {
            writeln!(f)?;
            writeln!(f, "{}", single.doc)?;
        }
        return Ok(());
    }

    writeln!(f, "fn {name}:")?;
    let merged = merge_overload_docs(variants);
    let sigs = variants.iter().map(|v| variant_sig(v)).collect::<Vec<_>>();
    let sig_width = sigs.iter().map(String::len).max().unwrap_or(0);

    for sig in sigs {
        if let Some((_, description)) = merged
            .descriptions
            .iter()
            .find(|(description_sig, _)| description_sig == &sig)
        {
            writeln!(f, "    {sig:<sig_width$}  {description}")?;
        } else {
            writeln!(f, "    {sig}")?;
        }
    }

    if !merged.examples.is_empty() {
        let examples = merge_example_blocks(&merged.examples);
        writeln!(f)?;
        writeln!(f, "## Examples")?;
        writeln!(f)?;
        writeln!(f, "```garden")?;
        writeln!(f, "{examples}")?;
        writeln!(f, "```")?;
    }

    Ok(())
}

struct MergedOverloadDocs {
    descriptions: Vec<(String, String)>,
    examples: Vec<String>,
}

fn merge_overload_docs(variants: &[&Fn<'_>]) -> MergedOverloadDocs {
    let mut descriptions = Vec::new();
    let mut examples = Vec::new();
    for v in variants {
        if v.doc.is_empty() {
            continue;
        }
        let (description, example) = split_doc_examples(v.doc);
        if !description.is_empty() {
            descriptions.push((variant_sig(v), description));
        }
        if !example.is_empty() {
            examples.push(example);
        }
    }
    MergedOverloadDocs {
        descriptions,
        examples,
    }
}

fn merge_example_blocks(examples: &[String]) -> String {
    let mut seen = Vec::<&str>::new();
    let mut out = Vec::new();
    for example in examples {
        for line in example.lines() {
            if !line.is_empty() && seen.contains(&line) {
                continue;
            }
            if line.is_empty() && !out.is_empty() {
                continue;
            }
            if !line.is_empty() {
                seen.push(line);
            }
            out.push(line);
        }
    }

    while out.last().is_some_and(|line| line.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

fn split_doc_examples(doc: &str) -> (String, String) {
    let Some((description, rest)) = doc.split_once("## Examples") else {
        return (doc.trim().to_owned(), String::new());
    };

    let example = rest
        .trim_start()
        .strip_prefix("```garden")
        .and_then(|rest| rest.strip_suffix("```"))
        .map_or_else(
            || rest.trim().to_owned(),
            |example| example.trim().to_owned(),
        );

    (description.trim().to_owned(), example)
}

fn print_overload_group_summary(
    name: &str,
    variants: &[&Fn<'_>],
    f: &mut dyn fmt::Write,
) -> fmt::Result {
    if let [single] = variants {
        return print_function_head(single, f);
    }

    writeln!(f, "fn {name}:")?;
    for v in variants {
        writeln!(f, "  {}", variant_sig(v))?;
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
                print_overload_group_summary(name, &variants, f)?;
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
