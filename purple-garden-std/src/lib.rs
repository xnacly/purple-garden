pub use purple_garden_runtime::{Fn, Pkg};

mod io;
mod strings;
mod testing;

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

pub static STD: &[Pkg] = &[
    io::PACKAGE,
    strings::PACKAGE,
    testing::PACKAGE,
];
