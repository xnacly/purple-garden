use proc_macro::TokenStream;

mod derives;
mod pkg;

#[proc_macro_attribute]
pub fn pg_pkg(attr: TokenStream, item: TokenStream) -> TokenStream {
    pkg::expand(attr, item)
}

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
