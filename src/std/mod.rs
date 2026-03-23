use std::fmt;

use crate::{ir::ptype::Type, vm::BuiltinFn};

mod io;
mod strings;

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
    pub args: &'static [Type],
    pub ret: Type,
}

// TODO: replace this with a tri or some kind of compile time perfect hashing so the repeated
// lookup in lowering and typechecking is a bit better

/// resolve_pkg searches for a package in the standard library by its name, for instance "io/fs",
/// "runtime/gc" or "encoding/json"
pub fn resolve_pkg(query: &str) -> Option<&Pkg> {
    let mut segments = query.split('/');

    let first = segments.next()?;
    let root = STD.iter().find(|p| p.name == first)?;

    segments.try_fold(root, |pkg, segment| {
        pkg.pkgs.iter().find(|p| p.name == segment)
    })
}

fn print_function_head(fun: &Fn, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "fn {}(", fun.name)?;
    for (i, a) in fun.args.iter().enumerate() {
        if i + 1 < fun.args.len() {
            write!(f, "{} ", a)?;
        } else {
            write!(f, "{}", a)?;
        }
    }
    writeln!(f, ") {}", fun.ret);
    Ok(())
}

impl fmt::Display for Fn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        print_function_head(self, f)?;
        writeln!(f, "\t{}", self.doc)?;
        Ok(())
    }
}

impl fmt::Display for Pkg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "import ({})\n", self.name)?;
        writeln!(f, "{}", self.doc)?;

        if !self.pkgs.is_empty() {
            writeln!(f, "");
            for p in self.pkgs {
                writeln!(f, "{}/{}", self.name, p.name)?;
            }
        }

        if !self.fns.is_empty() {
            writeln!(f, "");
            for fun in self.fns {
                print_function_head(fun, f)?;
            }
        }

        Ok(())
    }
}

pub static STD: &[Pkg] = &[
    Pkg {
        name: "io",
        doc: "Package io provides rudimentary I/O primitives,
like writing and reading from file descriptors",
        pkgs: &[],
        fns: &[
            Fn {
                name: "println",
                doc: "writes its argument to stdout, with a newline appended",
                ptr: crate::std::io::println,
                args: &[Type::Str],
                ret: Type::Void,
            },
            Fn {
                name: "print",
                ptr: crate::std::io::print,
                doc: "writes its argument to stdout",
                args: &[Type::Str],
                ret: Type::Void,
            },
        ],
    },
    Pkg {
        name: "strings",
        doc: "Package strings implementes function manipulating strings",
        pkgs: &[],
        fns: &[
            Fn {
                name: "contains",
                doc: "reports whether arg 1 is in arg 0",
                ptr: crate::std::strings::contains,
                args: &[Type::Str, Type::Str],
                ret: Type::Int,
            },
            Fn {
                name: "len",
                doc: "returns len of arg 0",
                ptr: crate::std::strings::len,
                args: &[Type::Str],
                ret: Type::Int,
            },
        ],
    },
];
