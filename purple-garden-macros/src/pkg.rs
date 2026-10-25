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
    with_slots: bool,
    intrinsic: Option<Ident>,
    args: Vec<FunctionArg>,
    ret: Type,
    raw: bool,
}

struct ExpandedFunction {
    wrapper: TokenStream2,
    native: Option<TokenStream2>,
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
            "pg_pkg only supports inline modules, e.g. `mod str { ... }`",
        )
        .to_compile_error()
        .into();
    };

    let mut generated_items = Vec::new();
    let mut metadata = Vec::new();
    let mut subpackages = Vec::new();
    let mut errors = TokenStream2::new();

    for item in items.iter_mut() {
        match item {
            Item::Fn(fun) => match expand_function(&api, fun) {
                Ok(expanded) => {
                    generated_items.push(Item::Verbatim(expanded.wrapper));
                    if let Some(native) = expanded.native {
                        generated_items.push(Item::Verbatim(native));
                    }
                    if let Some(const_eval) = expanded.const_eval {
                        generated_items.push(Item::Verbatim(const_eval));
                    }
                    metadata.push(expanded.metadata);
                }
                Err(err) => errors.extend(err.to_compile_error()),
            },
            Item::Mod(submodule) if has_pg_pkg_attr(&submodule.attrs) => {
                let ident = &submodule.ident;
                subpackages.push(quote!(#ident::PACKAGE));
            }
            _ => {}
        }
    }

    let pkg_name = package_name(&module.ident);
    let pkg_doc = doc_string(&module.attrs);
    let package = quote! {
        pub const PACKAGE: #api::embed::Pkg = #api::embed::Pkg {
            name: #pkg_name,
            doc: #pkg_doc,
            pkgs: &[#(#subpackages),*],
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

fn has_pg_pkg_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|seg| seg.ident == "pg_pkg")
    })
}

fn package_name(ident: &Ident) -> String {
    let raw = ident.to_string();
    raw.strip_prefix("r#").unwrap_or(&raw).to_owned()
}

fn parse_attr(attr: TokenStream) -> syn::Result<Path> {
    if attr.is_empty() {
        return Ok(syn::parse_quote!(::purple_garden));
    }

    let parser = Punctuated::<Meta, syn::Token![,]>::parse_terminated;
    let metas = parser.parse(attr)?;

    if let Some(meta) = metas.into_iter().next() {
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
        native: function.native(api),
        metadata: function.metadata(api, const_eval.is_some()),
        const_eval,
    })
}

impl PgFunction {
    fn parse(fun: &mut ItemFn) -> syn::Result<Self> {
        let ident = fun.sig.ident.clone();
        let ret = return_type(&fun.sig.output)?;
        let attrs = take_pg_fn_attrs(&mut fun.attrs)?;

        if attrs.raw && attrs.pure {
            return Err(syn::Error::new(
                ident.span(),
                "pg_fn(unsafe) functions cannot be marked pure",
            ));
        }

        if attrs.intrinsic.is_some() && attrs.specialises.is_some() {
            return Err(syn::Error::new(
                ident.span(),
                "pg_fn(intrinsic) functions are lowered instead of called, they cannot specialise an overload group",
            ));
        }

        if attrs.with_slots && attrs.specialises.is_some() {
            return Err(syn::Error::new(
                ident.span(),
                "pg_fn(with_slots) functions cannot specialise an overload group, slots are bound per call instead",
            ));
        }

        let args = if attrs.raw {
            parse_unsafe_args(&fun.sig.inputs)?
        } else {
            parse_args(&fun.sig.inputs)?
        };

        Ok(Self {
            wrapper: format_ident!("__pg_wrapper_{ident}"),
            name: ident.to_string(),
            doc: doc_string(&fun.attrs),
            pure: attrs.pure,
            specialises: attrs.specialises,
            args,
            with_slots: attrs.with_slots,
            intrinsic: attrs.intrinsic,
            ret,
            raw: attrs.raw,
            ident,
        })
    }

    fn wrapper(&self, api: &Path) -> TokenStream2 {
        let wrapper_name = &self.wrapper;
        let body = if self.raw {
            self.unsafe_wrapper_body(api)
        } else {
            self.wrapper_body(api)
        };

        quote! {
            unsafe extern "C" fn #wrapper_name(vm: *mut std::ffi::c_void) {
                // SAFETY: callers pass a valid, uniquely borrowed Vm
                let vm = unsafe { &mut *vm.cast::<#api::embed::Vm>() };
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
                // SAFETY: idx < arity, args fit the registers
                let #binding = <#ty as #api::embed::FromVm>::from_vm(vm, *unsafe { vm.r(#idx) });
            }
        })
    }

    fn wrapper_call(&self, api: &Path) -> TokenStream2 {
        if returns_unit(&self.ret) {
            self.call_unit_function()
        } else {
            self.call_value_function(api)
        }
    }

    fn unsafe_wrapper_body(&self, api: &Path) -> TokenStream2 {
        let decode_args = self.vm_arg_decoders(api);
        let call = self.unsafe_wrapper_call(api);

        quote! {
            #(#decode_args)*
            #call
        }
    }

    fn unsafe_wrapper_call(&self, api: &Path) -> TokenStream2 {
        if returns_unit(&self.ret) {
            self.call_unsafe_unit_function()
        } else {
            self.call_unsafe_value_function(api)
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
            let ret = <#ret_ty as #api::embed::IntoVm>::into_vm(ret, vm);
            *vm.r_mut(0) = ret;
        }
    }

    fn call_unsafe_unit_function(&self) -> TokenStream2 {
        let fn_name = &self.ident;
        let args = self.unsafe_arg_bindings();

        quote! {
            #fn_name(#(#args),*);
        }
    }

    fn call_unsafe_value_function(&self, api: &Path) -> TokenStream2 {
        let fn_name = &self.ident;
        let args = self.unsafe_arg_bindings();
        let ret_ty = &self.ret;

        quote! {
            let ret = #fn_name(#(#args),*);
            let ret = <#ret_ty as #api::embed::IntoVm>::into_vm(ret, vm);
            *vm.r_mut(0) = ret;
        }
    }

    /// The entry native code calls directly, see `NativeFn`: Rust `f64` arguments and result stay
    /// `f64`, everything else crosses as a `Value` word and is decoded like in the wrapper.
    fn native(&self, api: &Path) -> Option<TokenStream2> {
        if self.intrinsic.is_some() {
            return None;
        }
        let name = format_ident!("__pg_native_{}", self.ident);
        let fn_name = &self.ident;
        let params = self.args.iter().map(|arg| {
            let binding = &arg.binding;
            let ty = native_ty(&arg.ty, api);
            quote!(#binding: #ty)
        });
        let decoders = self.args.iter().filter(|arg| !is_f64(&arg.ty)).map(|arg| {
            let binding = &arg.binding;
            let ty = &arg.ty;
            quote!(let #binding = <#ty as #api::embed::FromVm>::from_vm(vm, #binding);)
        });
        let call_args: Vec<TokenStream2> = if self.raw {
            self.unsafe_arg_bindings()
        } else {
            self.arg_bindings()
                .map(|binding| quote!(#binding))
                .collect()
        };
        let ret_ty = &self.ret;
        let ret_sig = native_ret_sig(ret_ty, api);
        let call = if returns_unit(ret_ty) || is_f64(ret_ty) {
            quote!(#fn_name(#(#call_args),*))
        } else {
            quote! {
                let ret = #fn_name(#(#call_args),*);
                <#ret_ty as #api::embed::IntoVm>::into_vm(ret, vm)
            }
        };

        Some(quote! {
            #[doc(hidden)]
            #[allow(unused_variables)]
            unsafe extern "C" fn #name(vm: *mut std::ffi::c_void, #(#params),*) #ret_sig {
                // SAFETY: callers pass a valid, uniquely borrowed Vm
                let vm = unsafe { &mut *vm.cast::<#api::embed::Vm>() };
                #(#decoders)*
                #call
            }
        })
    }

    fn arg_bindings(&self) -> impl Iterator<Item = &Ident> {
        self.args.iter().map(|arg| &arg.binding)
    }

    fn unsafe_arg_bindings(&self) -> Vec<TokenStream2> {
        std::iter::once(quote!(vm))
            .chain(self.args.iter().map(|arg| {
                let binding = &arg.binding;
                quote!(#binding)
            }))
            .collect()
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
        let with_slots = self.with_slots;
        let intrinsic = match &self.intrinsic {
            Some(intrinsic) => quote!(Some(#api::embed::Intrinsic::#intrinsic)),
            None => quote!(None),
        };
        let ret_ty = &self.ret;
        let native = if self.intrinsic.is_some() {
            quote!(None)
        } else {
            let name = format_ident!("__pg_native_{}", self.ident);
            let arg_tys = self.args.iter().map(|arg| native_ty(&arg.ty, api));
            let ret_sig = native_ret_sig(ret_ty, api);
            quote! {
                // SAFETY: same size fn ptr, callers re-cast to this signature
                Some(unsafe {
                    ::core::mem::transmute::<
                        unsafe extern "C" fn(*mut ::std::ffi::c_void, #(#arg_tys),*) #ret_sig,
                        #api::embed::NativeFn,
                    >(#name)
                })
            }
        };
        let arg_names = self
            .args
            .iter()
            .map(|arg| LitStr::new(&arg.name, self.ident.span()));
        let arg_types = self.args.iter().map(|arg| &arg.ty);

        quote! {
            #api::embed::Fn {
                name: #name,
                doc: #doc,
                ptr: #wrapper_name,
                pure: #pure,
                eval: #eval_name,
                arg_names: &[#(#arg_names),*],
                args: &[#(<#arg_types as #api::embed::PgType>::TYPE),*],
                ret: <#ret_ty as #api::embed::PgType>::TYPE,
                specialises: #specialises,
                with_slots: #with_slots,
                intrinsic: #intrinsic,
                native: #native,
            }
        }
    }
}

fn parse_args(inputs: &Punctuated<FnArg, syn::Token![,]>) -> syn::Result<Vec<FunctionArg>> {
    parse_garden_args(inputs.iter().enumerate())
}

fn parse_unsafe_args(inputs: &Punctuated<FnArg, syn::Token![,]>) -> syn::Result<Vec<FunctionArg>> {
    let Some(first) = inputs.first() else {
        return Err(syn::Error::new(
            inputs.span(),
            "pg_fn(unsafe) functions must take `&mut Vm` as their first argument",
        ));
    };

    let FnArg::Typed(arg) = first else {
        return Err(syn::Error::new(
            first.span(),
            "pg_fn(unsafe) functions cannot take self",
        ));
    };

    let Type::Reference(reference) = arg.ty.as_ref() else {
        return Err(syn::Error::new(
            arg.ty.span(),
            "pg_fn(unsafe) first argument must be `&mut Vm`",
        ));
    };

    if reference.mutability.is_none() {
        return Err(syn::Error::new(
            reference.span(),
            "pg_fn(unsafe) first argument must be mutable",
        ));
    }

    parse_garden_args(inputs.iter().skip(1).enumerate())
}

fn parse_garden_args<'a>(
    args: impl Iterator<Item = (usize, &'a FnArg)>,
) -> syn::Result<Vec<FunctionArg>> {
    args.map(|(idx, input)| {
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

/// Builtins can't fail: a failure the caller can handle is an `Option`, the rest are traps the
/// runtime raises itself.
fn return_type(output: &ReturnType) -> syn::Result<Type> {
    match output {
        ReturnType::Default => Ok(syn::parse_quote!(())),
        ReturnType::Type(_, ty) => match ty.as_ref() {
            Type::Path(path)
                if path
                    .path
                    .segments
                    .last()
                    .is_some_and(|s| s.ident == "Result") =>
            {
                Err(syn::Error::new_spanned(
                    ty,
                    "pg_fn functions cannot return Result, return an Option instead",
                ))
            }
            ty => Ok(ty.clone()),
        },
    }
}

fn returns_unit(ty: &Type) -> bool {
    matches!(ty, Type::Tuple(tuple) if tuple.elems.is_empty())
}

/// Double crosses native entries as `f64` so SysV passes it in an xmm register.
fn is_f64(ty: &Type) -> bool {
    matches!(ty, Type::Path(path) if path.qself.is_none() && path.path.is_ident("f64"))
}

fn native_ty(ty: &Type, api: &Path) -> TokenStream2 {
    if is_f64(ty) {
        quote!(f64)
    } else {
        quote!(#api::embed::Value)
    }
}

fn native_ret_sig(ty: &Type, api: &Path) -> TokenStream2 {
    if returns_unit(ty) {
        quote!()
    } else {
        let ty = native_ty(ty, api);
        quote!(-> #ty)
    }
}

#[derive(Default)]
struct PgFnAttrs {
    /// `#[pg_fn(pure)]`: const-foldable, gets a `__pg_eval_*` companion.
    pure: bool,
    /// `#[pg_fn(unsafe)]`: passes `&mut Vm` as the first Rust argument while
    /// exposing only the remaining Rust arguments to Garden.
    raw: bool,
    /// `#[pg_fn(specialises = "group")]`: one variant of an overload group; the
    /// fn is reachable only via `group`, never its own name.
    specialises: Option<String>,
    /// `#[pg_fn(with_slots)]`: the signature uses `embed::Slot<"T">` generics, bound per call
    /// by the typechecker
    with_slots: bool,
    /// `#[pg_fn(intrinsic = Name)]`: lowered instead of being called, by the frontend or the jit,
    /// see `Intrinsic`
    intrinsic: Option<Ident>,
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
        self.raw |= other.raw;
        self.specialises = other.specialises.or_else(|| self.specialises.take());
        self.with_slots |= other.with_slots;
        self.intrinsic = other.intrinsic.or_else(|| self.intrinsic.take());
    }

    fn parse_option(&mut self, meta: Meta) -> syn::Result<()> {
        if meta.path().is_ident("pure") {
            self.pure = true;
            return Ok(());
        }

        if meta.path().is_ident("unsafe") {
            self.raw = true;
            return Ok(());
        }

        if meta.path().is_ident("specialises") {
            self.specialises = Some(parse_specialises_option(meta)?);
            return Ok(());
        }

        if meta.path().is_ident("with_slots") {
            self.with_slots = true;
            return Ok(());
        }

        if meta.path().is_ident("intrinsic") {
            self.intrinsic = Some(parse_intrinsic_option(meta)?);
            return Ok(());
        }

        Err(syn::Error::new(meta.span(), "unknown pg_fn option"))
    }
}

/// Strip and parse the `#[pg_fn(..)]` marker off a stdlib fn, leaving its other
/// attributes intact. Accepts `pure`, `unsafe`, `with_slots`, `intrinsic = Name` and
/// `specialises = "group"` in any order.
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

fn parse_intrinsic_option(meta: Meta) -> syn::Result<Ident> {
    let Meta::NameValue(name_value) = meta else {
        return Err(syn::Error::new(
            meta.span(),
            "`intrinsic` must be `intrinsic = Name`",
        ));
    };

    let syn::Expr::Path(path) = &name_value.value else {
        return Err(syn::Error::new(
            name_value.value.span(),
            "`intrinsic` must name an `Intrinsic` variant",
        ));
    };
    path.path.get_ident().cloned().ok_or_else(|| {
        syn::Error::new(
            name_value.value.span(),
            "`intrinsic` must name an `Intrinsic` variant",
        )
    })
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
        Type::Path(path) if path_is_ident(path, "Str") => Some(quote! {
            ::purple_garden_ir::Const::from(ret.as_str().to_owned())
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
