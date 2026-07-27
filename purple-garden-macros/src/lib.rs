use proc_macro::TokenStream;

mod derives;
mod pkg;

#[proc_macro_attribute]
/// Exports a Rust module as a Purple Garden package.
///
/// Public functions in the module become Garden-callable functions and the
/// macro adds a `PACKAGE` constant for [`purple_garden::Pg::with_lib`]. Use
/// [`pg_fn`] on individual functions when they need options such as `pure`,
/// `specialises`, or `unsafe`.
///
/// # Examples
///
/// ```rust,ignore
/// use purple_garden::{Pg, pg_pkg};
///
/// #[pg_pkg]
/// mod numbers {
///     pub fn double(value: i64) -> i64 {
///         value * 2
///     }
/// }
///
/// let mut program = Pg::new()
///     .with_lib(&numbers::PACKAGE)
///     .compile(br#"import "numbers"
/// numbers.double(21)"#)?;
/// assert_eq!(program.run_take::<i64>()?, 42);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn pg_pkg(attr: TokenStream, item: TokenStream) -> TokenStream {
    pkg::expand(attr, item)
}

/// Configures a function exported by `#[pg_pkg]`.
///
/// Supported options:
///
/// - `pure`: marks the function deterministic and side-effect-free so constant
///   calls can be folded by the optimizer.
/// - `specialises = "name"`: exports the function as an overload variant of
///   `name` instead of under its Rust function name.
/// - `unsafe`: passes `&mut Vm` as the first Rust argument while exposing only
///   the remaining arguments to Garden. The wrapper still decodes those
///   remaining arguments and encodes the return value normally.
///
/// `pure` and `unsafe` are mutually exclusive. Options can otherwise be
/// combined, for example `#[pg_fn(unsafe, specialises = "stats")]`.
///
/// # Examples
///
/// ```rust,ignore
/// use purple_garden::{pg_fn, pg_pkg};
///
/// #[pg_pkg]
/// mod math {
///     #[pg_fn(pure)]
///     pub fn double(value: i64) -> i64 {
///         value * 2
///     }
///
///     #[pg_fn(specialises = "show")]
///     pub fn show_int(value: i64) -> String {
///         value.to_string()
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn pg_fn(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Derives embedding for a Rust type represented as a first-class Garden value.
///
/// The type becomes an anonymous Garden record. Every named field must itself
/// be convertible to and from a Garden value. The derive implements `PgType`,
/// `IntoVm`, and `FromVm`.
///
/// # Examples
///
/// ```rust,ignore
/// use purple_garden::GardenValue;
///
/// #[derive(GardenValue)]
/// struct RetryConfig {
///     attempts: i64,
///     backoff_ms: i64,
/// }
///
/// #[derive(GardenValue)]
/// struct AppConfig {
///     service: String,
///     retry: RetryConfig,
/// }
/// ```
#[proc_macro_derive(GardenValue)]
pub fn derive_garden_value(item: TokenStream) -> TokenStream {
    derives::garden_value(item)
}

/// Derives foreign-handle embedding for an opaque Rust-owned struct.
///
/// The Garden type is `Foreign<Name>` and Garden code can pass the value back
/// to Rust but cannot inspect its fields. Use this for resources, handles, and
/// mutable Rust-owned state. The derive implements `PgType`, `IntoVm`, and
/// `FromVm` for borrowed references to the type.
///
/// # Examples
///
/// ```rust,ignore
/// use std::sync::atomic::AtomicI64;
/// use purple_garden::GardenOpaque;
///
/// #[derive(GardenOpaque)]
/// struct Counter {
///     value: AtomicI64,
/// }
/// ```
#[proc_macro_derive(GardenOpaque)]
pub fn derive_garden_opaque(item: TokenStream) -> TokenStream {
    derives::garden_opaque(item)
}
