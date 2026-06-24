use std::{collections::HashMap, sync::OnceLock};

pub use purple_garden_runtime::{Fn, Pkg};

mod io;
mod math;
mod strings;
mod testing;

/// `resolve_pkg` searches for a package in the standard library by its name, for instance "io/fs",
/// "runtime/gc" or "encoding/json"
#[must_use]
pub fn resolve_pkg(query: &str) -> Option<&Pkg> {
    std_pkg_index().get(query).copied()
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

pub static STD: &[Pkg] = &[
    io::PACKAGE,
    math::PACKAGE,
    strings::PACKAGE,
    testing::PACKAGE,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_root_packages() {
        assert_eq!(resolve_pkg("io").unwrap().name, "io");
        assert_eq!(resolve_pkg("math").unwrap().name, "math");
        assert_eq!(resolve_pkg("strings").unwrap().name, "strings");
        assert_eq!(resolve_pkg("testing").unwrap().name, "testing");
    }

    #[test]
    fn rejects_unknown_or_malformed_paths() {
        assert!(resolve_pkg("").is_none());
        assert!(resolve_pkg("missing").is_none());
        assert!(resolve_pkg("io/").is_none());
        assert!(resolve_pkg("io/missing").is_none());
    }
}
