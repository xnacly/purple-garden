extern crate self as purple_garden;

pub use purple_garden_runtime::{
    Field, Fn, FromVm, IntoVm, PgType, Pkg, RecordFields, Type, Value, Vm,
};

#[doc(hidden)]
pub mod embed {
    pub use super::{Field, Fn, FromVm, IntoVm, PgType, Pkg, RecordFields, Type, Value, Vm};
    pub use purple_garden_runtime::{
        alloc_record, copy_record, decode_record_field, encode_record_field,
    };
}

mod meta;
mod raylib;

pub use meta::META_PACKAGE;
pub use raylib::RAYLIB_PACKAGE;

const EXP_PACKAGE: Pkg = Pkg {
    name: "exp",
    doc: "Experimental dependency-backed packages.",
    pkgs: &[RAYLIB_PACKAGE],
    fns: &[],
};

pub const PACKAGE: Pkg = Pkg {
    name: "vendor",
    doc: "Dependency-backed and experimental packages.",
    pkgs: &[EXP_PACKAGE, META_PACKAGE],
    fns: &[],
};

/// Packages provided by optional, dependency-backed integrations.
pub static PACKAGES: &[Pkg] = &[PACKAGE];
