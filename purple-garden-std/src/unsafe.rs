pub use self::r#unsafe::PACKAGE;

/// Package unsafe provides inherently unsafe operations.
#[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
pub mod r#unsafe {

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

    /// Package syscall provides low level linux syscall interaction
    #[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
    pub mod syscall {}
}
