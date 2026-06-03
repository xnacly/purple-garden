use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Data, DeriveInput, GenericParam, Generics, Lifetime, LifetimeParam, Path, parse_macro_input,
    spanned::Spanned,
};

fn runtime_path() -> Path {
    syn::parse_quote!(::purple_garden)
}

pub fn pg_type(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let api = runtime_path();
    expand_pg_type(&api, &input).into()
}

pub fn into_vm(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let api = runtime_path();
    match expand_into_vm(&api, &input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

pub fn from_vm(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let api = runtime_path();
    match expand_from_vm(&api, &input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand_pg_type(api: &Path, input: &DeriveInput) -> TokenStream2 {
    let ident = &input.ident;
    let foreign = ident.to_string();
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics #api::PgType for #ident #ty_generics #where_clause {
            const TYPE: #api::Type<'static> = #api::Type::Foreign(#foreign);
        }
    }
}

fn expand_into_vm(api: &Path, input: &DeriveInput) -> syn::Result<TokenStream2> {
    struct_only(input)?;

    let ident = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    Ok(quote! {
        impl #impl_generics #api::IntoVm for #ident #ty_generics #where_clause {
            fn into_vm(self, _: &mut #api::Vm) -> #api::Value {
                #api::Value::from_ptr(Box::into_raw(Box::new(self)))
            }
        }
    })
}

fn expand_from_vm(api: &Path, input: &DeriveInput) -> syn::Result<TokenStream2> {
    struct_only(input)?;

    let ident = &input.ident;
    let mut generics = input.generics.clone();
    prepend_lifetime(&mut generics, "vm");
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (_, ty_generics, _) = input.generics.split_for_impl();

    Ok(quote! {
        impl #impl_generics #api::FromVm<'vm> for &'vm #ident #ty_generics #where_clause {
            fn from_vm(vm: &'vm #api::Vm, idx: usize) -> Self {
                unsafe { &*vm.r(idx).as_ptr::<#ident #ty_generics>() }
            }
        }

        impl #impl_generics #api::FromVm<'vm> for &'vm mut #ident #ty_generics #where_clause {
            fn from_vm(vm: &'vm #api::Vm, idx: usize) -> Self {
                unsafe { &mut *vm.r(idx).as_ptr::<#ident #ty_generics>() }
            }
        }
    })
}

fn struct_only(input: &DeriveInput) -> syn::Result<()> {
    if matches!(input.data, Data::Struct(_)) {
        Ok(())
    } else {
        Err(syn::Error::new(
            input.span(),
            "foreign VM derives currently support structs only",
        ))
    }
}

fn prepend_lifetime(generics: &mut Generics, name: &str) {
    let lifetime = Lifetime::new(&format!("'{name}"), generics.span());
    generics
        .params
        .insert(0, GenericParam::Lifetime(LifetimeParam::new(lifetime)));
}
