macro_rules! safe_syscall_invocation {
    ($call:path, $raw:ty, $error:expr) => {{
        let mut raw = std::mem::MaybeUninit::<$raw>::uninit();

        if unsafe { $call(raw.as_mut_ptr()) } != 0 {
            return Err($error);
        }

        Ok(unsafe { raw.assume_init() }.into())
    }};
}

macro_rules! syscalls {
    (
        $(
            $(#[$meta:meta])*
            ($name:ident, $raw:ident, ($($field:ident: [$char:ty; $len:literal]),+ $(,)?))
        ),+ $(,)?
    ) => {
        /// Package syscall provides low level linux syscall interaction
        #[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
        pub mod syscall {
            mod __pg_syscall_ffi {
                unsafe extern "C" {
                    $($(#[$meta])* pub(super) fn $name(buf: *mut super::$raw) -> std::ffi::c_int;)+
                }
            }

            $(
                $(#[$meta])*
                #[allow(non_camel_case_types)]
                #[derive(Clone, Debug, Eq, PartialEq, purple_garden_macros::GardenValue)]
                pub struct $name {
                    $(pub $field: String),+
                }

                $(#[$meta])*
                #[repr(C)]
                struct $raw {
                    $($field: [$char; $len]),+
                }

                $(#[$meta])*
                impl From<$raw> for $name {
                    fn from(raw: $raw) -> Self {
                        $(let $field = unsafe { std::ffi::CStr::from_ptr(raw.$field.as_ptr()) }
                            .to_string_lossy()
                            .into_owned();)+

                        Self { $($field),+ }
                    }
                }

                $(#[$meta])*
                #[purple_garden_macros::pg_fn(unsafe)]
                pub fn $name(_vm: &mut purple_garden_runtime::Vm) -> Result<$name, &'static str> {
                    safe_syscall_invocation!(
                        __pg_syscall_ffi::$name,
                        $raw,
                        concat!(stringify!($name), " syscall failed")
                    )
                }
            )+
        }
    };
}
