use std::{collections::HashMap, sync::OnceLock};

extern crate self as purple_garden;

pub use purple_garden_runtime::{
    Anomaly, Field, Fn, FromVm, IntoVm, Intrinsic, PgType, Pkg, Type, Value, Vm, alloc_record,
    copy_record, decode_record_field, encode_record_field,
};

/// Compatibility namespace used by embedding macros.
#[doc(hidden)]
pub mod embed {
    pub use super::{
        Anomaly, Field, Fn, FromVm, IntoVm, Intrinsic, PgType, Pkg, Type, Value, Vm, alloc_record,
        copy_record, decode_record_field, encode_record_field,
    };
}

mod io;
mod math;
mod opt;
mod str;
mod testing;
mod r#unsafe;

/// `resolve_pkg` searches for a package in the standard library by its name, for instance "io/fs",
/// "runtime/gc" or "encoding/json"
#[must_use]
pub fn resolve_pkg(query: &str) -> Option<&Pkg> {
    std_pkg_index().get(query).copied()
}

#[must_use]
pub fn resolve_pkg_in<'pkg>(pkgs: &'pkg [Pkg], query: &str) -> Option<&'pkg Pkg> {
    let (head, tail) = query.split_once('/').unwrap_or((query, ""));
    let pkg = pkgs.iter().find(|pkg| pkg.name == head)?;

    if tail.is_empty() {
        return Some(pkg);
    }

    resolve_pkg_in(pkg.pkgs, tail)
}

fn std_pkg_index() -> &'static HashMap<String, &'static Pkg> {
    static INDEX: OnceLock<HashMap<String, &'static Pkg>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index = HashMap::new();
        for pkg in STD {
            insert_pkg(&mut index, String::new(), pkg);
        }
        index
    })
}

fn insert_pkg(index: &mut HashMap<String, &'static Pkg>, parent: String, pkg: &'static Pkg) {
    let path = if parent.is_empty() {
        pkg.name.to_owned()
    } else {
        format!("{parent}/{}", pkg.name)
    };

    index.insert(path.clone(), pkg);

    for sub in pkg.pkgs {
        insert_pkg(index, path.clone(), sub);
    }
}

pub static SAFE_STD: &[Pkg] = &[
    io::PACKAGE,
    math::PACKAGE,
    opt::PACKAGE,
    str::PACKAGE,
    testing::PACKAGE,
];

pub static STD: &[Pkg] = &[
    io::PACKAGE,
    math::PACKAGE,
    opt::PACKAGE,
    str::PACKAGE,
    testing::PACKAGE,
    r#unsafe::PACKAGE,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_root_packages() {
        assert_eq!(resolve_pkg("io").unwrap().name, "io");
        assert_eq!(resolve_pkg("math").unwrap().name, "math");
        assert_eq!(resolve_pkg("str").unwrap().name, "str");
        assert_eq!(resolve_pkg("testing").unwrap().name, "testing");
        assert_eq!(resolve_pkg("unsafe").unwrap().name, "unsafe");
    }

    #[test]
    fn resolves_nested_packages() {
        let pkg = resolve_pkg("unsafe/runtime").unwrap();

        assert_eq!(pkg.name, "runtime");
    }

    #[test]
    fn rejects_unknown_or_malformed_paths() {
        assert!(resolve_pkg("").is_none());
        assert!(resolve_pkg("missing").is_none());
        assert!(resolve_pkg("strings").is_none());
        assert!(resolve_pkg("io/").is_none());
        assert!(resolve_pkg("io/missing").is_none());
        assert!(resolve_pkg("unsafe/").is_none());
        assert!(resolve_pkg("unsafe/missing").is_none());
    }
}
