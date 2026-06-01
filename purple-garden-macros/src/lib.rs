use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn pg_pkg(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn pg_fn(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
