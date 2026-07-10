use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Data, DeriveInput, Fields, FieldsNamed, GenericParam, Generics, Lifetime, LifetimeParam, Path,
    parse_macro_input, spanned::Spanned,
};

fn runtime_path() -> Path {
    syn::parse_quote!(::purple_garden)
}

pub fn garden_value(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let api = runtime_path();

    expand_garden_value(&api, &input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

pub fn garden_opaque(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let api = runtime_path();

    expand_garden_opaque(&api, &input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand_garden_value(api: &Path, input: &DeriveInput) -> syn::Result<TokenStream2> {
    let fields = named_fields(input)?;

    let pg_type = expand_record_pg_type(api, input, fields);
    let from_vm = expand_record_from_vm(api, input, fields);
    let into_vm = expand_record_into_vm(api, input, fields);

    Ok(quote! {
        #pg_type
        #from_vm
        #into_vm
    })
}

fn expand_record_into_vm(api: &Path, input: &DeriveInput, fields: &FieldsNamed) -> TokenStream2 {
    let ident = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let encoders = fields.named.iter().map(|field| record_encoder(api, field));

    quote! {
        impl #impl_generics #api::IntoVm for #ident #ty_generics #where_clause {
            fn into_vm(self, vm: &mut #api::Vm) -> #api::Value {
                let record = #api::alloc_record(vm, &<Self as #api::PgType>::TYPE);
                #(#encoders)*
                record
            }
        }
    }
}

fn record_encoder(api: &Path, field: &syn::Field) -> TokenStream2 {
    let field_ident = field.ident.as_ref().unwrap();
    let field_name = field_ident.to_string();
    quote! {
        #api::encode_record_field(vm, record, &<Self as #api::PgType>::TYPE, #field_name, self.#field_ident);
    }
}

fn expand_record_from_vm(api: &Path, input: &DeriveInput, fields: &FieldsNamed) -> TokenStream2 {
    let ident = &input.ident;
    let mut generics = input.generics.clone();
    prepend_lifetime(&mut generics, "vm");
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (_, ty_generics, _) = input.generics.split_for_impl();
    let decoders = record_decoders(api, fields, quote!(&<Self as #api::PgType>::TYPE));

    quote! {
        impl #impl_generics #api::FromVm<'vm> for #ident #ty_generics #where_clause {
            fn from_vm(vm: &'vm #api::Vm, base: #api::Value) -> Self {
                Self { #(#decoders),* }
            }
        }
    }
}

fn record_decoders(api: &Path, fields: &FieldsNamed, record_ty: TokenStream2) -> Vec<TokenStream2> {
    fields
        .named
        .iter()
        .map(|field| {
            let field_ident = field.ident.as_ref().unwrap();
            let field_name = field_ident.to_string();
            let ty = &field.ty;
            quote! {
                #field_ident: unsafe {
                    #api::decode_record_field::<#ty>(vm, base, #record_ty, #field_name)
                }
            }
        })
        .collect()
}

fn expand_garden_opaque(api: &Path, input: &DeriveInput) -> syn::Result<TokenStream2> {
    struct_only(input)?;

    let ident = &input.ident;
    let foreign = ident.to_string();
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let mut from_generics = input.generics.clone();
    prepend_lifetime(&mut from_generics, "vm");
    let (from_impl_generics, _, from_where_clause) = from_generics.split_for_impl();

    Ok(quote! {
        impl #impl_generics #api::PgType for #ident #ty_generics #where_clause {
            const TYPE: #api::Type<'static> = #api::Type::Foreign(#foreign);
        }

        impl #impl_generics #api::IntoVm for #ident #ty_generics #where_clause {
            fn into_vm(self, _: &mut #api::Vm) -> #api::Value {
                #api::Value::from_ptr(Box::into_raw(Box::new(self)))
            }
        }

        impl #from_impl_generics #api::FromVm<'vm> for &'vm #ident #ty_generics #from_where_clause {
            fn from_vm(_: &'vm #api::Vm, value: #api::Value) -> Self {
                unsafe { &*value.as_ptr::<#ident #ty_generics>() }
            }
        }

        impl #from_impl_generics #api::FromVm<'vm> for &'vm mut #ident #ty_generics #from_where_clause {
            fn from_vm(_: &'vm #api::Vm, value: #api::Value) -> Self {
                unsafe { &mut *value.as_ptr::<#ident #ty_generics>() }
            }
        }
    })
}

fn expand_record_pg_type(api: &Path, input: &DeriveInput, fields: &FieldsNamed) -> TokenStream2 {
    let ident = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let record_fields = fields.named.iter().map(|field| record_field(api, field));

    quote! {
        impl #impl_generics #api::PgType for #ident #ty_generics #where_clause {
            const TYPE: #api::Type<'static> = #api::Type::Record(
                #api::RecordFields::Static(&[#(#record_fields),*])
            );
        }
    }
}

fn record_field(api: &Path, field: &syn::Field) -> TokenStream2 {
    let field_ident = field.ident.as_ref().unwrap();
    let field_name = field_ident.to_string();
    let ty = &field.ty;
    quote! {
        #api::Field { name: #field_name, ty: <#ty as #api::PgType>::TYPE }
    }
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

fn named_fields(input: &DeriveInput) -> syn::Result<&FieldsNamed> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "record VM derives currently support structs only",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new(
            input.span(),
            "record VM derives require named struct fields",
        ));
    };
    Ok(fields)
}

fn prepend_lifetime(generics: &mut Generics, name: &str) {
    let lifetime = Lifetime::new(&format!("'{name}"), generics.span());
    generics
        .params
        .insert(0, GenericParam::Lifetime(LifetimeParam::new(lifetime)));
}
