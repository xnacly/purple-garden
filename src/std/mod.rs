use crate::{ir::ptype::Type, vm::BuiltinFn};

mod io;

pub struct Pkg {
    name: &'static str,
    pkgs: &'static [Pkg],
    fns: &'static [Fn],
}

pub struct Fn {
    name: &'static str,
    ptr: BuiltinFn<'static>,
    args: &'static [Type],
    ret: Type,
}

pub static STD: &[Pkg] = &[Pkg {
    name: "io",
    pkgs: &[],
    fns: &[
        Fn {
            name: "println",
            ptr: crate::std::io::println,
            args: &[Type::Str],
            ret: Type::Void,
        },
        Fn {
            name: "print",
            ptr: crate::std::io::print,
            args: &[Type::Str],
            ret: Type::Void,
        },
    ],
}];
