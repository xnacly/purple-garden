use purple_garden_macros::GardenValue;
use purple_garden_runtime::Pkg;

#[derive(GardenValue)]
pub struct Versions {
    pub raylib: String,
}

#[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
/// Metadata for dependency-backed vendor packages.
pub mod meta {
    use super::Versions;

    /// Returns the versions of the linked vendor dependencies.
    #[purple_garden_macros::pg_fn(pure)]
    pub fn versions() -> Versions {
        Versions {
            raylib: "6.0".to_owned(),
        }
    }
}

pub const META_PACKAGE: Pkg = meta::PACKAGE;
