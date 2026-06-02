use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    parse::Parser, parse_macro_input, punctuated::Punctuated, spanned::Spanned, Attribute, FnArg,
    Ident, Item, ItemFn, ItemMod, LitStr, Meta, Pat, Path, ReturnType, Type,
};

struct Arg {
    name: String,
    binding: Ident,
    ty: Type,
}

struct Function {
    ident: Ident,
    wrapper: Ident,
    name: String,
    doc: String,
    pure: bool,
    args: Vec<Arg>,
    ret: Type,
    result: bool,
}

pub fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let api = match parse_attr(attr) {
        Ok(api) => api,
        Err(err) => return err.to_compile_error().into(),
    };
    let mut module = parse_macro_input!(item as ItemMod);

    let Some((_, items)) = module.content.as_mut() else {
        return syn::Error::new(
            module.span(),
            "pg_pkg only supports inline modules, e.g. `mod strings { ... }`",
        )
        .to_compile_error()
        .into();
    };

    let mut generated = Vec::new();
    let mut metadata = Vec::new();
    let mut errors = TokenStream2::new();

    for item in items.iter_mut() {
        let Item::Fn(fun) = item else {
            continue;
        };

        match expand_function(&api, fun) {
            Ok((wrapper, meta)) => {
                generated.push(Item::Verbatim(wrapper));
                metadata.push(meta);
            }
            Err(err) => errors.extend(err.to_compile_error()),
        }
    }

    let pkg_name = module.ident.to_string();
    let pkg_doc = doc_string(&module.attrs);
    let package = quote! {
        pub const PACKAGE: #api::Pkg = #api::Pkg {
            name: #pkg_name,
            doc: #pkg_doc,
            pkgs: &[],
            fns: &[#(#metadata),*],
        };
    };

    items.extend(generated);
    items.push(Item::Verbatim(package));

    quote! {
        #module
        #errors
    }
    .into()
}

fn parse_attr(attr: TokenStream) -> syn::Result<Path> {
    if attr.is_empty() {
        return Ok(syn::parse_quote!(::purple_garden));
    }

    let parser = Punctuated::<Meta, syn::Token![,]>::parse_terminated;
    let metas = parser.parse(attr)?;

    for meta in metas {
        let Meta::NameValue(name_value) = meta else {
            return Err(syn::Error::new(meta.span(), "expected `runtime = path`"));
        };
        if !name_value.path.is_ident("runtime") {
            return Err(syn::Error::new(
                name_value.path.span(),
                "unknown pg_pkg option",
            ));
        }
        let syn::Expr::Path(expr_path) = name_value.value else {
            return Err(syn::Error::new(
                name_value.value.span(),
                "`runtime` must be a path",
            ));
        };
        return Ok(expr_path.path);
    }

    Ok(syn::parse_quote!(::purple_garden))
}

fn expand_function(api: &Path, fun: &mut ItemFn) -> syn::Result<(TokenStream2, TokenStream2)> {
    let function = Function::parse(fun)?;
    Ok((function.wrapper(api), function.metadata(api)))
}

impl Function {
    fn parse(fun: &mut ItemFn) -> syn::Result<Self> {
        let ident = fun.sig.ident.clone();
        let (ret, result) = return_type(&fun.sig.output);
        Ok(Self {
            wrapper: format_ident!("__pg_wrapper_{ident}"),
            name: ident.to_string(),
            doc: doc_string(&fun.attrs),
            pure: take_pg_fn_pure(&mut fun.attrs)?,
            args: parse_args(&fun.sig.inputs)?,
            ret,
            result,
            ident,
        })
    }

    fn wrapper(&self, api: &Path) -> TokenStream2 {
        let wrapper_name = &self.wrapper;
        let fn_name = &self.ident;
        let arg_bindings = self.args.iter().enumerate().map(|(idx, arg)| {
            let binding = &arg.binding;
            let ty = &arg.ty;
            quote! {
                let #binding = <#ty as #api::FromVm>::from_vm(vm, #idx);
            }
        });
        let arg_exprs = self.args.iter().map(|arg| &arg.binding);
        let ret_ty = &self.ret;
        let body = match (self.result, returns_unit(ret_ty)) {
            (false, true) => quote! {
                #(#arg_bindings)*
                #fn_name(#(#arg_exprs),*);
            },
            (false, false) => quote! {
                #(#arg_bindings)*
                let ret = #fn_name(#(#arg_exprs),*);
                let ret = <#ret_ty as #api::IntoVm>::into_vm(ret, vm);
                *vm.r_mut(0) = ret;
            },
            (true, true) => quote! {
                #(#arg_bindings)*
                if let Err(msg) = #fn_name(#(#arg_exprs),*) {
                    vm.trap(#api::Anomaly::Msg { msg, pc: vm.pc });
                }
            },
            (true, false) => quote! {
                #(#arg_bindings)*
                match #fn_name(#(#arg_exprs),*) {
                    Ok(ret) => {
                        let ret = <#ret_ty as #api::IntoVm>::into_vm(ret, vm);
                        *vm.r_mut(0) = ret;
                    }
                    Err(msg) => vm.trap(#api::Anomaly::Msg { msg, pc: vm.pc }),
                }
            },
        };

        quote! {
            unsafe extern "C" fn #wrapper_name(vm: *mut #api::Vm) {
                let vm = unsafe { &mut *vm };
                #body
            }
        }
    }

    fn metadata(&self, api: &Path) -> TokenStream2 {
        let wrapper_name = &self.wrapper;
        let name = &self.name;
        let doc = &self.doc;
        let pure = self.pure;
        let ret_ty = &self.ret;
        let arg_names = self
            .args
            .iter()
            .map(|arg| LitStr::new(&arg.name, self.ident.span()));
        let arg_types = self.args.iter().map(|arg| &arg.ty);

        quote! {
            #api::Fn {
                name: #name,
                doc: #doc,
                ptr: #wrapper_name,
                pure: #pure,
                arg_names: &[#(#arg_names),*],
                args: &[#(<#arg_types as #api::PgType>::TYPE),*],
                ret: <#ret_ty as #api::PgType>::TYPE,
            }
        }
    }
}

fn parse_args(inputs: &Punctuated<FnArg, syn::Token![,]>) -> syn::Result<Vec<Arg>> {
    inputs
        .iter()
        .enumerate()
        .map(|(idx, input)| {
            let FnArg::Typed(arg) = input else {
                return Err(syn::Error::new(
                    input.span(),
                    "pg_pkg functions cannot take self",
                ));
            };
            let Pat::Ident(pat_ident) = arg.pat.as_ref() else {
                return Err(syn::Error::new(
                    arg.pat.span(),
                    "pg_pkg function arguments must be simple identifiers",
                ));
            };

            Ok(Arg {
                name: pat_ident.ident.to_string(),
                binding: format_ident!("arg{idx}"),
                ty: arg.ty.as_ref().clone(),
            })
        })
        .collect()
}

fn return_type(output: &ReturnType) -> (Type, bool) {
    match output {
        ReturnType::Default => (syn::parse_quote!(()), false),
        ReturnType::Type(_, ty) => result_ok_type(ty)
            .map(|ok| (ok, true))
            .unwrap_or_else(|| (ty.as_ref().clone(), false)),
    }
}

fn result_ok_type(ty: &Type) -> Option<Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let segment = type_path.path.segments.last()?;
    if segment.ident != "Result" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    let Some(syn::GenericArgument::Type(ok)) = args.args.first() else {
        return None;
    };
    Some(ok.clone())
}

fn returns_unit(ty: &Type) -> bool {
    matches!(ty, Type::Tuple(tuple) if tuple.elems.is_empty())
}

fn take_pg_fn_pure(attrs: &mut Vec<Attribute>) -> syn::Result<bool> {
    let mut pure = false;
    let mut out = Vec::with_capacity(attrs.len());

    for attr in attrs.drain(..) {
        if !path_ends_with(attr.path(), "pg_fn") {
            out.push(attr);
            continue;
        }

        let metas = attr.parse_args_with(Punctuated::<Meta, syn::Token![,]>::parse_terminated)?;
        for meta in metas {
            if meta.path().is_ident("pure") {
                pure = true;
            } else {
                return Err(syn::Error::new(meta.span(), "unknown pg_fn option"));
            }
        }
    }

    *attrs = out;
    Ok(pure)
}

fn path_ends_with(path: &Path, ident: &str) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == ident)
}

fn doc_string(attrs: &[Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|attr| match &attr.meta {
            Meta::NameValue(name_value) if name_value.path.is_ident("doc") => {
                let syn::Expr::Lit(expr_lit) = &name_value.value else {
                    return None;
                };
                let syn::Lit::Str(lit) = &expr_lit.lit else {
                    return None;
                };
                Some(lit.value().trim().to_string())
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
