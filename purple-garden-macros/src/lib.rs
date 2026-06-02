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

/// Derives the Purple Garden type metadata for a Rust struct.
///
/// Structs derive to `Type::Foreign<StructName>`. Primitive types already
/// implement this in the runtime.
#[proc_macro_derive(PgType)]
pub fn derive_pg_type(item: TokenStream) -> TokenStream {
    derives::pg_type(item)
}

/// Derives return-value encoding for a foreign Rust struct.
///
/// The current encoding boxes the value and stores its pointer as a VM foreign
/// handle. Use with `#[derive(PgType, FromVm)]` for roundtripping.
#[proc_macro_derive(IntoVm)]
pub fn derive_into_vm(item: TokenStream) -> TokenStream {
    derives::into_vm(item)
}

/// Derives argument decoding for foreign Rust structs.
///
/// Generates `FromVm` for `&T` and `&mut T`, so package functions can take
/// normal Rust references while wrappers read the VM foreign handle.
#[proc_macro_derive(FromVm)]
pub fn derive_from_vm(item: TokenStream) -> TokenStream {
    derives::from_vm(item)
}
