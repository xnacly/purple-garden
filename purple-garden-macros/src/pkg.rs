use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Attribute, FnArg, Ident, Item, ItemFn, ItemMod, LitStr, Meta, Pat, Path, ReturnType, Type,
    parse::Parser, parse_macro_input, punctuated::Punctuated, spanned::Spanned,
};

struct FunctionArg {
    name: String,
    binding: Ident,
    ty: Type,
}

struct PgFunction {
    ident: Ident,
    wrapper: Ident,
    name: String,
    doc: String,
    pure: bool,
    specialises: Option<String>,
    args: Vec<FunctionArg>,
    ret: Type,
    result: bool,
}

struct ExpandedFunction {
    wrapper: TokenStream2,
    const_eval: Option<TokenStream2>,
    metadata: TokenStream2,
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

    let mut generated_items = Vec::new();
    let mut metadata = Vec::new();
    let mut errors = TokenStream2::new();

    for item in items.iter_mut() {
        let Item::Fn(fun) = item else {
            continue;
        };

        match expand_function(&api, fun) {
            Ok(expanded) => {
                generated_items.push(Item::Verbatim(expanded.wrapper));
                if let Some(const_eval) = expanded.const_eval {
                    generated_items.push(Item::Verbatim(const_eval));
                }
                metadata.push(expanded.metadata);
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

    generated_items.push(Item::Verbatim(package));
    items.extend(generated_items);

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

fn expand_function(api: &Path, fun: &mut ItemFn) -> syn::Result<ExpandedFunction> {
    let function = PgFunction::parse(fun)?;
    let const_eval = function.const_eval()?;

    Ok(ExpandedFunction {
        wrapper: function.wrapper(api),
        metadata: function.metadata(api, const_eval.is_some()),
        const_eval,
    })
}

impl PgFunction {
    fn parse(fun: &mut ItemFn) -> syn::Result<Self> {
        let ident = fun.sig.ident.clone();
        let (ret, result) = return_type(&fun.sig.output);
        let attrs = take_pg_fn_attrs(&mut fun.attrs)?;
        Ok(Self {
            wrapper: format_ident!("__pg_wrapper_{ident}"),
            name: ident.to_string(),
            doc: doc_string(&fun.attrs),
            pure: attrs.pure,
            specialises: attrs.specialises,
            args: parse_args(&fun.sig.inputs)?,
            ret,
            result,
            ident,
        })
    }

    fn wrapper(&self, api: &Path) -> TokenStream2 {
        let wrapper_name = &self.wrapper;
        let body = self.wrapper_body(api);

        quote! {
            unsafe extern "C" fn #wrapper_name(vm: *mut std::ffi::c_void) {
                let vm = unsafe { &mut *vm.cast::<#api::Vm>() };
                #body
            }
        }
    }

    fn wrapper_body(&self, api: &Path) -> TokenStream2 {
        let decode_args = self.vm_arg_decoders(api);
        let call = self.wrapper_call(api);

        quote! {
            #(#decode_args)*
            #call
        }
    }

    fn vm_arg_decoders<'a>(&'a self, api: &'a Path) -> impl Iterator<Item = TokenStream2> + 'a {
        self.args.iter().enumerate().map(move |(idx, arg)| {
            let binding = &arg.binding;
            let ty = &arg.ty;

            quote! {
                let #binding = <#ty as #api::FromVm>::from_vm(vm, #idx);
            }
        })
    }

    fn wrapper_call(&self, api: &Path) -> TokenStream2 {
        match (self.result, returns_unit(&self.ret)) {
            (false, true) => self.call_unit_function(),
            (false, false) => self.call_value_function(api),
            (true, true) => self.call_unit_result_function(api),
            (true, false) => self.call_value_result_function(api),
        }
    }

    fn call_unit_function(&self) -> TokenStream2 {
        let fn_name = &self.ident;
        let args = self.arg_bindings();

        quote! {
            #fn_name(#(#args),*);
        }
    }

    fn call_value_function(&self, api: &Path) -> TokenStream2 {
        let fn_name = &self.ident;
        let args = self.arg_bindings();
        let ret_ty = &self.ret;

        quote! {
            let ret = #fn_name(#(#args),*);
            let ret = <#ret_ty as #api::IntoVm>::into_vm(ret, vm);
            *vm.r_mut(0) = ret;
        }
    }

    fn call_unit_result_function(&self, api: &Path) -> TokenStream2 {
        let fn_name = &self.ident;
        let args = self.arg_bindings();

        quote! {
            if let Err(msg) = #fn_name(#(#args),*) {
                vm.trap(#api::Anomaly::Msg { msg, pc: vm.pc });
            }
        }
    }

    fn call_value_result_function(&self, api: &Path) -> TokenStream2 {
        let fn_name = &self.ident;
        let args = self.arg_bindings();
        let ret_ty = &self.ret;

        quote! {
            match #fn_name(#(#args),*) {
                Ok(ret) => {
                    let ret = <#ret_ty as #api::IntoVm>::into_vm(ret, vm);
                    *vm.r_mut(0) = ret;
                }
                Err(msg) => vm.trap(#api::Anomaly::Msg { msg, pc: vm.pc }),
            }
        }
    }

    fn arg_bindings(&self) -> impl Iterator<Item = &Ident> {
        self.args.iter().map(|arg| &arg.binding)
    }

    fn const_eval(&self) -> syn::Result<Option<TokenStream2>> {
        if !self.pure {
            return Ok(None);
        }

        let eval_name = format_ident!("__pg_eval_{}", self.ident);
        let fn_name = &self.ident;
        let argc = self.args.len();
        let mut arg_decoders = Vec::with_capacity(self.args.len());
        for (idx, arg) in self.args.iter().enumerate() {
            let Some(expr) = const_arg_expr(&arg.ty, idx) else {
                return Ok(None);
            };
            let binding = &arg.binding;
            arg_decoders.push(quote! {
                let #binding = #expr;
            });
        }
        let arg_exprs = self.arg_bindings();
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
                #(#arg_decoders)*
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
        let specialises = match &self.specialises {
            Some(group) => quote!(Some(#group)),
            None => quote!(None),
        };
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
                specialises: #specialises,
            }
        }
    }
}

fn parse_args(inputs: &Punctuated<FnArg, syn::Token![,]>) -> syn::Result<Vec<FunctionArg>> {
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

            Ok(FunctionArg {
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

#[derive(Default)]
struct PgFnAttrs {
    /// `#[pg_fn(pure)]`: const-foldable, gets a `__pg_eval_*` companion.
    pure: bool,
    /// `#[pg_fn(specialises = "group")]`: one variant of an overload group; the
    /// fn is reachable only via `group`, never its own name.
    specialises: Option<String>,
}

impl PgFnAttrs {
    fn parse(attr: &Attribute) -> syn::Result<Self> {
        let mut attrs = Self::default();

        let metas = attr.parse_args_with(Punctuated::<Meta, syn::Token![,]>::parse_terminated)?;
        for meta in metas {
            attrs.parse_option(meta)?;
        }

        Ok(attrs)
    }

    fn merge(&mut self, other: Self) {
        self.pure |= other.pure;
        self.specialises = other.specialises.or_else(|| self.specialises.take());
    }

    fn parse_option(&mut self, meta: Meta) -> syn::Result<()> {
        if meta.path().is_ident("pure") {
            self.pure = true;
            return Ok(());
        }

        if meta.path().is_ident("specialises") {
            self.specialises = Some(parse_specialises_option(meta)?);
            return Ok(());
        }

        Err(syn::Error::new(meta.span(), "unknown pg_fn option"))
    }
}

/// Strip and parse the `#[pg_fn(..)]` marker off a stdlib fn, leaving its other
/// attributes intact. Accepts `pure` and `specialises = "group"` in any order.
fn take_pg_fn_attrs(attrs: &mut Vec<Attribute>) -> syn::Result<PgFnAttrs> {
    let mut pg_fn_attrs = PgFnAttrs::default();
    let mut out = Vec::with_capacity(attrs.len());

    for attr in attrs.drain(..) {
        if !path_ends_with(attr.path(), "pg_fn") {
            out.push(attr);
            continue;
        }

        pg_fn_attrs.merge(PgFnAttrs::parse(&attr)?);
    }

    *attrs = out;
    Ok(pg_fn_attrs)
}

fn parse_specialises_option(meta: Meta) -> syn::Result<String> {
    let Meta::NameValue(name_value) = meta else {
        return Err(syn::Error::new(
            meta.span(),
            "`specialises` must be `specialises = \"group\"`",
        ));
    };

    let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(group),
        ..
    }) = name_value.value
    else {
        return Err(syn::Error::new(
            name_value.value.span(),
            "`specialises` must be a string literal",
        ));
    };

    Ok(group.value())
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
