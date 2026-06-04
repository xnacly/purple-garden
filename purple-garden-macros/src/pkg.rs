use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Attribute, FnArg, Ident, Item, ItemFn, ItemMod, LitStr, Meta, Pat, Path, ReturnType, Type,
    parse::Parser, parse_macro_input, punctuated::Punctuated, spanned::Spanned,
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
            Ok((wrapper, eval, meta)) => {
                generated.push(Item::Verbatim(wrapper));
                if let Some(eval) = eval {
                    generated.push(Item::Verbatim(eval));
                }
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

fn expand_function(
    api: &Path,
    fun: &mut ItemFn,
) -> syn::Result<(TokenStream2, Option<TokenStream2>, TokenStream2)> {
    let function = Function::parse(fun)?;
    let eval = function.const_eval()?;
    Ok((
        function.wrapper(api),
        eval.clone(),
        function.metadata(api, eval.is_some()),
    ))
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
            unsafe extern "C" fn #wrapper_name(vm: *mut std::ffi::c_void) {
                let vm = unsafe { &mut *vm.cast::<#api::Vm>() };
                #body
            }
        }
    }

    fn const_eval(&self) -> syn::Result<Option<TokenStream2>> {
        if !self.pure {
            return Ok(None);
        }

        let eval_name = format_ident!("__pg_eval_{}", self.ident);
        let fn_name = &self.ident;
        let argc = self.args.len();
        let mut arg_bindings = Vec::with_capacity(self.args.len());
        for (idx, arg) in self.args.iter().enumerate() {
            let Some(expr) = const_arg_expr(&arg.ty, idx) else {
                return Ok(None);
            };
            let binding = &arg.binding;
            arg_bindings.push(quote! {
                let #binding = #expr;
            });
        }
        let arg_exprs = self.args.iter().map(|arg| &arg.binding);
        let Some(ret_expr) = const_ret_expr(&self.ret) else {
            return Ok(None);
        };

        Ok(Some(quote! {
            fn #eval_name<'args, 'c>(
                args: &'args [::purple_garden_ir::Const<'c>],
            ) -> Option<::purple_garden_ir::Const<'c>> {
                if args.len() != #argc {
                    return None;
                }
                #(#arg_bindings)*
                let ret = #fn_name(#(#arg_exprs),*);
                Some(#ret_expr)
            }
        }))
    }

    fn metadata(&self, api: &Path, has_eval: bool) -> TokenStream2 {
        let wrapper_name = &self.wrapper;
        let eval_name = if has_eval {
            let ident = format_ident!("__pg_eval_{}", self.ident);
            quote!(Some(#ident))
        } else {
            quote!(None)
        };
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
                eval: #eval_name,
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

fn const_arg_expr(ty: &Type, idx: usize) -> Option<TokenStream2> {
    match ty {
        Type::Path(path) if path_is_ident(path, "bool") => Some(quote! {
            match args.get(#idx)? {
                ::purple_garden_ir::Const::True => true,
                ::purple_garden_ir::Const::False => false,
                _ => return None,
            }
        }),
        Type::Path(path) if path_is_ident(path, "i64") => Some(quote! {
            match args.get(#idx)? {
                ::purple_garden_ir::Const::Int(v) => *v,
                _ => return None,
            }
        }),
        Type::Path(path) if path_is_ident(path, "f64") => Some(quote! {
            match args.get(#idx)? {
                ::purple_garden_ir::Const::Double(v) => f64::from_bits(*v),
                _ => return None,
            }
        }),
        Type::Path(path) if path_is_ident(path, "String") => Some(quote! {
            match args.get(#idx)? {
                ::purple_garden_ir::Const::Str(v) => v.clone().into_owned(),
                _ => return None,
            }
        }),
        Type::Reference(reference) if is_str_ref(reference) => Some(quote! {
            match args.get(#idx)? {
                ::purple_garden_ir::Const::Str(v) => v.as_ref(),
                _ => return None,
            }
        }),
        _ => None,
    }
}

fn const_ret_expr(ty: &Type) -> Option<TokenStream2> {
    match ty {
        Type::Path(path) if path_is_ident(path, "bool") => Some(quote! {
            ::purple_garden_ir::Const::from(ret)
        }),
        Type::Path(path) if path_is_ident(path, "i64") => Some(quote! {
            ::purple_garden_ir::Const::from(ret)
        }),
        Type::Path(path) if path_is_ident(path, "f64") => Some(quote! {
            ::purple_garden_ir::Const::from(ret)
        }),
        Type::Path(path) if path_is_ident(path, "String") => Some(quote! {
            ::purple_garden_ir::Const::from(ret)
        }),
        Type::Reference(reference) if is_str_ref(reference) => Some(quote! {
            ::purple_garden_ir::Const::from(ret)
        }),
        _ => None,
    }
}

fn path_is_ident(path: &syn::TypePath, ident: &str) -> bool {
    path.qself.is_none()
        && path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == ident)
}

fn is_str_ref(reference: &syn::TypeReference) -> bool {
    match reference.elem.as_ref() {
        Type::Path(path) => path_is_ident(path, "str"),
        _ => false,
    }
}
