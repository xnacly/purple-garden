use proc_macro::TokenStream;

mod derives;
mod pkg;

#[proc_macro_attribute]
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
#[proc_macro_attribute]
pub fn pg_fn(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Derives embedding for a Rust type represented as a first-class Garden value.
#[proc_macro_derive(GardenValue)]
pub fn derive_garden_value(item: TokenStream) -> TokenStream {
    derives::garden_value(item)
}

/// Derives foreign-handle embedding for an opaque Rust-owned struct.
#[proc_macro_derive(GardenOpaque)]
pub fn derive_garden_opaque(item: TokenStream) -> TokenStream {
    derives::garden_opaque(item)
}
