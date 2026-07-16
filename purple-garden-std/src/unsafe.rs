pub use self::r#unsafe::PACKAGE;

/// Package unsafe provides inherently unsafe operations.
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

    #[cfg(target_os = "linux")]
    syscalls! {
        /// Returns system identity information from uname(2).
        (uname, RawUtsName, (
            sysname: [std::ffi::c_char; 65],
            nodename: [std::ffi::c_char; 65],
            release: [std::ffi::c_char; 65],
            version: [std::ffi::c_char; 65],
            machine: [std::ffi::c_char; 65],
            domainname: [std::ffi::c_char; 65],
        )),
    }

    #[cfg(target_os = "macos")]
    syscalls! {
        /// Returns system identity information from uname(2).
        (uname, RawUtsName, (
            sysname: [std::ffi::c_char; 256],
            nodename: [std::ffi::c_char; 256],
            release: [std::ffi::c_char; 256],
            version: [std::ffi::c_char; 256],
            machine: [std::ffi::c_char; 256],
        )),
    }

    pub const PACKAGE: Pkg = Pkg {
        name: "unsafe",
        doc: "Package unsafe provides inherently unsafe operations.",
        pkgs: &[runtime::PACKAGE, syscall::PACKAGE],
        fns: &[] as &[Fn<'static>],
    };
}
