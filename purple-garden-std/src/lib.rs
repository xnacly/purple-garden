use std::fmt;

use purple_garden_ir::ptype::Type;
use purple_garden_runtime::BuiltinFn;

macro_rules! builtin {
    ($(pub fn $name:ident($vm:ident) $body:block)*) => {
        $(
            pub unsafe extern "C" fn $name($vm: *mut purple_garden_runtime::Vm) {
                let $vm = unsafe { &mut *$vm };
                $body
            }
        )*
    };
}

pub(crate) use builtin;

mod conv;
mod io;
mod strings;
mod testing;

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

// TODO: replace this with a tri or some kind of compile time perfect hashing so the repeated
// lookup in lowering and typechecking is a bit better

/// `resolve_pkg` searches for a package in the standard library by its name, for instance "io/fs",
/// "runtime/gc" or "encoding/json"
#[must_use]
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

pub static STD: &[Pkg] = &[
    Pkg {
        name: "io",
        doc: "Package io provides rudimentary I/O primitives,
like writing and reading from file descriptors",
        pkgs: &[],
        fns: &[
            Fn {
                name: "println",
                doc: "writes s to stdout, with a newline appended",
                ptr: crate::io::println,
                pure: false,
                arg_names: &["s"],
                args: &[Type::Str],
                ret: Type::Void,
            },
            Fn {
                name: "print",
                ptr: crate::io::print,
                doc: "writes s to stdout",
                pure: false,
                arg_names: &["s"],
                args: &[Type::Str],
                ret: Type::Void,
            },
        ],
    },
    Pkg {
        name: "strings",
        doc: "Package strings implements functions manipulating strings",
        pkgs: &[],
        fns: &[
            Fn {
                name: "contains",
                doc: "reports whether needle appears in hay",
                ptr: crate::strings::contains,
                pure: true,
                arg_names: &["hay", "needle"],
                args: &[Type::Str, Type::Str],
                ret: Type::Bool,
            },
            Fn {
                name: "repeat",
                doc: "repeats s n times",
                ptr: crate::strings::repeat,
                pure: false,
                arg_names: &["s", "n"],
                args: &[Type::Str, Type::Int],
                ret: Type::Str,
            },
            Fn {
                name: "len",
                doc: "returns the length of s in bytes",
                ptr: crate::strings::len,
                pure: true,
                arg_names: &["s"],
                args: &[Type::Str],
                ret: Type::Int,
            },
        ],
    },
    Pkg {
        name: "conv",
        doc: "Package conv includes helpers for roundtripping various datatypes",
        pkgs: &[],
        fns: &[
            Fn {
                name: "from_int",
                doc: "converts n to Str",
                ptr: crate::conv::from_int,
                pure: true,
                arg_names: &["n"],
                args: &[Type::Int],
                ret: Type::Str,
            },
            Fn {
                name: "from_double",
                doc: "converts d to Str",
                ptr: crate::conv::from_double,
                pure: true,
                arg_names: &["d"],
                args: &[Type::Double],
                ret: Type::Str,
            },
        ],
    },
    Pkg {
        name: "testing",
        doc: "Package testing includes helpers for runtime assertions and the likes",
        pkgs: &[],
        fns: &[Fn {
            name: "assert",
            doc: "asserts condition is true",
            ptr: crate::testing::assert,
            pure: false,
            arg_names: &["condition"],
            args: &[Type::Bool],
            ret: Type::Void,
        }],
    },
];
