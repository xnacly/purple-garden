pub use self::r#unsafe::PACKAGE;

/// Package unsafe provides inherently unsafe operations.
// This module is the public `unsafe` package namespace; its file name matches it.
#[allow(clippy::module_inception)]
pub mod r#unsafe {
    use purple_garden_runtime::{Fn, Pkg};

    /// Package runtime provides methods to interact with purple-gardens runtime
    #[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
    pub mod runtime {
        /// Returns the amount of used bytes
        #[purple_garden_macros::pg_fn(unsafe)]
        pub fn used(vm: &mut purple_garden_runtime::Vm) -> i64 {
            vm.gc.total_used() as i64
        }

        /// Returns the amount of allocated bytes
        #[purple_garden_macros::pg_fn(unsafe)]
        pub fn allocs(vm: &mut purple_garden_runtime::Vm) -> i64 {
            vm.gc.total_alloc() as i64
        }
    }

    pub const PACKAGE: Pkg = Pkg {
        name: "unsafe",
        doc: "Package unsafe provides inherently unsafe operations.",
        pkgs: &[runtime::PACKAGE],
        fns: &[] as &[Fn<'static>],
    };
}
