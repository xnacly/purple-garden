#![allow(clippy::result_large_err)]

mod display;
mod err;
mod typedefs;

use std::collections::HashMap;

use purple_garden_frontend::typemap::{TypeMap, TypeRef};
use purple_garden_frontend::{
    ast::{Ast, Node, NodeId},
    diagnostic::{Diagnostic, Span},
    lex::{self, Token},
};
use purple_garden_ir::ptype::{BoxedType, Type};
use purple_garden_runtime::Pkg;
use purple_garden_std as pstd;

pub use typedefs::FunctionType;
pub use typedefs::TypecheckOutput;
use typedefs::{CallName, CallSink, TcType};

#[derive(Debug)]
pub struct Typechecker<'a, 't> {
    ast: &'a Ast<'t>,
    /// Node id -> Type. Indexed by id; Node ids are dense from the parser, the slot past the last
    /// id holds `Void` for results without a node of their own, see [`Self::void`]
    map: TypeMap<'t>,
    void: usize,
    /// scope stack; innermost frame last; lookups walk from top to bottom. Bindings point at
    /// the arena entry of the expression that produced them
    env: Vec<HashMap<&'t str, TypeRef>>,
    /// map a function name to its type(s)
    functions: HashMap<&'t str, FunctionType<'t>>,
    /// map a pkg name to a map of its public method names to overload groups
    /// (one entry per specialisation; >1 means a `specialises` group)
    packages: HashMap<&'t str, HashMap<&'t str, Vec<FunctionType<'t>>>>,
    pkg_cache: HashMap<&'t str, Option<&'t Pkg>>,
    libs: Vec<&'t Pkg>,
    stdlib: &'t [Pkg],
    diagnostics: Vec<Diagnostic>,
}

impl<'a, 't> Typechecker<'a, 't> {
    #[must_use]
    pub fn new(ast: &'a Ast<'t>) -> Self {
        let mut s = Self {
            ast,
            map: TypeMap::with_slots(ast.values + 1),
            void: ast.values,
            env: Vec::new(),
            functions: HashMap::new(),
            packages: HashMap::new(),
            pkg_cache: HashMap::new(),
            libs: Vec::new(),
            stdlib: pstd::STD,
            diagnostics: Vec::new(),
        };
        s.map.insert(s.void, Type::Void);
        s.env.push(HashMap::new());
        s
    }

    #[must_use]
    pub fn with_libs(mut self, libs: Vec<&'t Pkg>) -> Self {
        self.libs = libs;
        self
    }

    #[must_use]
    pub fn with_stdlib_enabled(mut self, stdlib: bool) -> Self {
        self.stdlib = if stdlib { pstd::STD } else { &[] };
        self
    }

    #[must_use]
    pub fn with_stdlib(mut self, stdlib: &'t [Pkg]) -> Self {
        self.stdlib = stdlib;
        self
    }

    fn env_get(&self, k: &str) -> Option<TypeRef> {
        self.env
            .iter()
            .rev()
            .find_map(|frame| frame.get(k))
            .copied()
    }

    fn env_insert(&mut self, k: &'t str, v: TypeRef) {
        self.env.last_mut().unwrap().insert(k, v);
    }

    fn resolve_pkg(&mut self, query: &'t str) -> Option<&'t Pkg> {
        if let Some(pkg) = self.pkg_cache.get(query).copied() {
            return pkg;
        }

        let pkg = self
            .libs
            .iter()
            .copied()
            .find(|pkg| pkg.name == query)
            .or_else(|| pstd::resolve_pkg_in(self.stdlib, query));

        self.pkg_cache.insert(query, pkg);
        pkg
    }

    fn register_pkg(&mut self, pkg: &'t Pkg) {
        let mut registered: HashMap<&str, Vec<FunctionType>> = HashMap::new();
        for f in pkg.fns {
            let f_type = FunctionType {
                args: f
                    .arg_names
                    .iter()
                    .copied()
                    .zip(f.args.iter().cloned())
                    .collect(),
                ret: f.ret.clone(),
                with_slots: f.with_slots,
            };
            registered.entry(f.group_name()).or_default().push(f_type);
        }

        self.packages.insert(pkg.name, registered);
    }

    fn register_extern(&mut self, node: NodeId) {
        let Node::Extern { name, fns, .. } = self.ast.node(node) else {
            return;
        };
        let lex::Type::S(pkg_name) = name.t else {
            unreachable!();
        };

        let mut registered: HashMap<&str, Vec<FunctionType>> = HashMap::new();
        for fun in fns {
            let lex::Type::Ident(fun_name) = fun.name.t else {
                unreachable!();
            };
            let args = fun
                .args
                .iter()
                .map(|(arg_name, arg_type)| {
                    let lex::Type::Ident(arg_name) = arg_name.t else {
                        unreachable!();
                    };
                    (
                        arg_name,
                        purple_garden_frontend::type_from_type_expr(self.ast, *arg_type),
                    )
                })
                .collect();
            let f_type = FunctionType {
                args,
                ret: purple_garden_frontend::type_from_type_expr(self.ast, fun.return_type),
                with_slots: false,
            };
            purple_garden_shared::trace!(
                "[ir::typecheck::Typechecker::extern][{}.{}]: {}",
                pkg_name,
                fun_name,
                f_type
            );
            registered.entry(fun_name).or_default().push(f_type);
        }

        self.packages.insert(pkg_name, registered);
    }

    #[must_use]
    pub fn check(mut self) -> TypecheckOutput<'t> {
        for &node in &self.ast.roots {
            self.node(node);
        }

        TypecheckOutput {
            types: self.map,
            functions: self.functions,
            diagnostics: self.diagnostics,
        }
    }

    fn set_known(&mut self, id: usize, t: Type<'t>) -> TcType {
        Self::store_known(&mut self.map, id, t)
    }

    #[inline]
    fn store_known(map: &mut TypeMap<'t>, id: usize, t: Type<'t>) -> TcType {
        map.insert(id, t);
        TcType::Known(id)
    }

    /// `id` shares the type already stored for `of`
    fn alias(&mut self, id: usize, of: usize) -> TcType {
        let r = self.map.ref_of(of).expect("known ids always have a type");
        self.map.bind(id, r);
        TcType::Known(id)
    }

    /// Type behind a [`TcType::Known`] id
    fn ty(&self, id: usize) -> &Type<'t> {
        self.map.get(id).expect("known ids always have a type")
    }

    fn already_checked(&self, node: NodeId) -> Option<TcType> {
        let id = self.ast.value_id(node);
        self.map.get(id).is_some().then_some(TcType::Known(id))
    }

    /// Type already assigned to `node`, by reference. Callers must have typed
    /// `node` first (via [`Self::node`]); used to read arg types for overload
    /// selection without cloning them out of the map.
    fn resolved_arg_ty(&self, node: NodeId) -> &Type<'t> {
        self.map
            .get(self.ast.value_id(node))
            .expect("arg typed before overload selection")
    }

    fn node_label(&self, node: NodeId) -> Option<&'t str> {
        match self.ast.node(node) {
            Node::Ident {
                name:
                    lex::Token {
                        t: lex::Type::Ident(name),
                        ..
                    },
                ..
            } => Some(*name),
            _ => None,
        }
    }

    fn fuse(op: &lex::Token, lhs: &Type<'t>, rhs: &Type<'t>) -> Result<Type<'t>, Diagnostic> {
        let ty = match op.t {
            // arithmetics
            lex::Type::Plus | lex::Type::Minus | lex::Type::Asteriks | lex::Type::Slash => {
                match (lhs, rhs) {
                    (Type::Int, Type::Int) => Type::Int,
                    (Type::Double, Type::Double) => Type::Double,
                    (_, _) if lhs == rhs => {
                        return Err(Diagnostic::at_token(
                            format!(
                                "Unsupported type {} for {:?}, want Int or Double",
                                lhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                    }
                    (_, _) => {
                        return Err(Diagnostic::at_token(
                            format!(
                                "Incompatible types {} and {} for {:?}",
                                lhs,
                                rhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                    }
                }
            }
            lex::Type::Percent => match (lhs, rhs) {
                (Type::Int, Type::Int) => Type::Int,
                (_, _) if lhs == rhs => {
                    return Err(Diagnostic::at_token(
                        format!("Unsupported type {} for {:?}, want Int", lhs, op.t.as_str()),
                        op,
                    ));
                }
                (_, _) => {
                    return Err(Diagnostic::at_token(
                        format!(
                            "Incompatible types {} and {} for {:?}, want both sides Int",
                            lhs,
                            rhs,
                            op.t.as_str()
                        ),
                        op,
                    ));
                }
            },
            lex::Type::DoubleEqual | lex::Type::NotEqual => {
                match (lhs, rhs) {
                    (Type::Int, Type::Int) | (Type::Bool, Type::Bool) => {}
                    (_, _) if lhs == rhs => {
                        return Err(Diagnostic::at_token(
                            format!(
                                "Unsupported type {} for {:?}, want Int or Bool",
                                lhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                    }
                    (_, _) => {
                        return Err(Diagnostic::at_token(
                            format!(
                                "Incompatible types {} and {} for {:?}, want both sides Int or both sides Bool",
                                lhs,
                                rhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                    }
                }
                Type::Bool
            }
            lex::Type::LessThan | lex::Type::GreaterThan => {
                match (lhs, rhs) {
                    (Type::Double, Type::Double) | (Type::Int, Type::Int) => {}
                    (_, _) if lhs == rhs => {
                        return Err(Diagnostic::at_token(
                            format!(
                                "Unsupported type {} for {:?}, want Int or Double for both sides",
                                lhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                    }
                    (_, _) => {
                        return Err(Diagnostic::at_token(
                            format!(
                                "Incompatible types {} and {} for {:?}",
                                lhs,
                                rhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                    }
                }
                Type::Bool
            }

            // lex::Type::Exclaim => todo!(),
            // lex::Type::Question => todo!(),
            _ => unreachable!(),
        };
        Ok(ty)
    }

    fn cast(&mut self, at: &lex::Token, lhs: usize, o: Type<'t>) -> Option<Type<'t>> {
        let i = self.ty(lhs);
        match (i, &o) {
            (Type::Int, Type::Double) => Some(Type::Double),
            (Type::Double | Type::Bool, Type::Int) => Some(Type::Int),
            (Type::Int, Type::Bool) => Some(Type::Bool),
            (_, _) if i == &o => {
                // This is still an error in PG, but the expression's type is
                // unambiguous. Keeping it known prevents downstream false
                // positives and makes `--types` more useful.
                let err = Self::redundant_cast_error(at, i);
                self.report(err);
                Some(o)
            }
            (_, _) => {
                let err = Diagnostic::at_token(format!("Can not cast {i} to {o}"), at);
                self.report(err);
                None
            }
        }
    }

    fn block_type(&mut self, nodes: &[NodeId]) -> TcType {
        self.env.push(HashMap::new());
        let mut last_type = TcType::Known(self.void);
        for &node in nodes {
            last_type = self.node(node);
        }
        self.env.pop();
        last_type
    }

    fn resolve_pkg_call(
        &mut self,
        call_id: usize,
        field_target: NodeId,
        name: &lex::Token,
        args: &[NodeId],
    ) -> TcType {
        let Node::Ident { name: pkg_tok, .. } = self.ast.node(field_target) else {
            self.report(Diagnostic::at_token(
                "only package functions can be called through field syntax",
                name,
            ));
            return TcType::Poison;
        };
        let lex::Token {
            t: lex::Type::Ident(pkg_name),
            ..
        } = pkg_tok
        else {
            unreachable!();
        };
        let lex::Token {
            t: lex::Type::Ident(inner_name),
            ..
        } = name
        else {
            unreachable!();
        };

        let mut args_poisoned = false;
        for &arg in args {
            if self.node(arg).known().is_none() {
                args_poisoned = true;
            }
        }

        let Some(pkg) = self.packages.get(pkg_name) else {
            let err = self.missing_package_error(pkg_name, pkg_tok);
            self.report(err);
            return TcType::Poison;
        };

        let Some(candidates) = pkg.get(inner_name) else {
            self.report(Diagnostic::at_token(
                format!("Call to undefined function `{pkg_name}.{inner_name}`"),
                name,
            ));
            return TcType::Poison;
        };

        // function call specialisation via monomorphic dispatch
        if candidates.len() > 1 {
            // If argument expressions were poisoned, exact overload
            // selection is impossible. A shared return type is still
            // useful enough to recover with.
            if args_poisoned {
                if let Some(ret) = Self::common_return(candidates) {
                    return Self::store_known(&mut self.map, call_id, ret);
                }
                return TcType::Poison;
            }

            let provided = || args.iter().map(|&a| self.resolved_arg_ty(a));

            let Some(idx) = candidates.iter().position(|c| {
                purple_garden_frontend::overload_matches(c.args.iter().map(|(_, t)| t), provided())
            }) else {
                let err =
                    self.specialisation_miss_error(pkg_name, inner_name, name, args, candidates);
                self.diagnostics.push(err);
                // `str.from(Str)` is invalid, but every variant returns
                // `Str`, so callers can still typecheck against that
                // result.
                if let Some(ret) = Self::common_return(candidates) {
                    return Self::store_known(&mut self.map, call_id, ret);
                }
                return TcType::Poison;
            };

            let ret = candidates[idx].ret.clone();
            purple_garden_shared::trace!(
                "[ir::typecheck::Typechecker::node] resolved `{}.{}` to specialisation {}/{} ({}) -> {}",
                pkg_name,
                inner_name,
                idx + 1,
                candidates.len(),
                provided()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                ret
            );
            return Self::store_known(&mut self.map, call_id, ret);
        }

        Self::check_call_args(
            CallSink {
                ast: self.ast,
                map: &mut self.map,
                diagnostics: &mut self.diagnostics,
            },
            call_id,
            name,
            &CallName {
                pkg: Some(pkg_name),
                name: inner_name,
            },
            &candidates[0],
            args,
        )
    }

    fn resolve_call(&mut self, call_id: usize, name: &lex::Token, args: &[NodeId]) -> TcType {
        let lex::Token {
            t: lex::Type::Ident(inner_name),
            ..
        } = name
        else {
            unreachable!();
        };

        for &arg in args {
            self.node(arg);
        }

        let Some(fun) = self.functions.get(inner_name) else {
            self.report(Diagnostic::at_token(
                format!("Call to undefined function `{inner_name}`"),
                name,
            ));
            return TcType::Poison;
        };

        Self::check_call_args(
            CallSink {
                ast: self.ast,
                map: &mut self.map,
                diagnostics: &mut self.diagnostics,
            },
            call_id,
            name,
            &CallName {
                pkg: None,
                name: inner_name,
            },
            fun,
            args,
        )
    }

    /// Arguments must already be typed, see [`CallSink`] for why `fun` is borrowed
    fn check_call_args(
        sink: CallSink<'_, 'a, 't>,
        call_id: usize,
        tok: &lex::Token,
        display_name: &CallName<'_>,
        fun: &FunctionType<'t>,
        args: &[NodeId],
    ) -> TcType {
        let CallSink {
            ast,
            map,
            diagnostics,
        } = sink;

        if args.len() != fun.args.len() {
            diagnostics.push(Diagnostic::at_token(
                format!(
                    "`{}` requires {} arguments, got {}",
                    display_name,
                    fun.args.len(),
                    args.len()
                ),
                tok,
            ));
            return Self::store_known(map, call_id, fun.ret.clone());
        }

        // PERF: replace with scratch storage
        let mut slot_to_type_bindings: HashMap<&str, Type<'_>> = HashMap::new();

        for (i, &provided_node) in args.iter().enumerate() {
            let Some(provided_type) = map.get(ast.value_id(provided_node)) else {
                continue;
            };

            let (expected_arg_name, expected_arg_type) = &fun.args[i];

            let span = ast
                .span(provided_node)
                .unwrap_or_else(|| Span::from_token(tok));

            if !fun.with_slots {
                if expected_arg_type != provided_type {
                    diagnostics.push(Self::arg_mismatch(
                        display_name,
                        expected_arg_name,
                        expected_arg_type,
                        provided_type,
                        span,
                    ));
                }
                continue;
            }

            if let Err(err) =
                expected_arg_type.bind_slots(provided_type, &mut slot_to_type_bindings)
            {
                diagnostics.push(Self::slot_bind_error(display_name, err, span));
                continue;
            }

            let expected_arg_type = expected_arg_type.apply_slot_binding(&slot_to_type_bindings);
            if *expected_arg_type != *provided_type {
                diagnostics.push(Self::arg_mismatch(
                    display_name,
                    expected_arg_name,
                    &expected_arg_type,
                    provided_type,
                    span,
                ));
            }
        }

        let ret = if fun.with_slots {
            fun.ret
                .apply_slot_binding(&slot_to_type_bindings)
                .into_owned()
        } else {
            fun.ret.clone()
        };

        Self::store_known(map, call_id, ret)
    }

    fn node(&mut self, node_id: NodeId) -> TcType {
        let node = self.ast.node(node_id);
        if let Some(known) = self.already_checked(node_id) {
            return known;
        }

        match node {
            Node::Array { id, members, src } => {
                let Some(first_member) = members.first() else {
                    self.report(
                        Diagnostic::at_token(
                            "Can not infer the element type of an empty array",
                            src,
                        )
                        .with_primary_message("empty array"),
                    );
                    return TcType::Poison;
                };

                let Some(first) = self.node(*first_member).known() else {
                    for member in members.iter().skip(1) {
                        self.node(*member);
                    }
                    return TcType::Poison;
                };

                let mut poisoned = false;
                for member in members.iter().skip(1) {
                    let Some(ty) = self.node(*member).known() else {
                        poisoned = true;
                        continue;
                    };
                    if self.ty(ty) != self.ty(first) {
                        let span = self
                            .ast
                            .span(*member)
                            .unwrap_or_else(|| Span::from_token(src));
                        let (first_type, ty) = (self.ty(first), self.ty(ty));
                        let err = Diagnostic::new(
                            format!(
                                "Array members must all have the same type, expected {first_type} but got {ty}"
                            ),
                            span,
                        )
                        .with_primary_message(format!("this member is of type {ty}"))
                        .with_note(format!(
                            "the array element type was inferred as {first_type} from the first member"
                        ));
                        self.report(err);
                        return TcType::Poison;
                    }
                }

                if poisoned {
                    return TcType::Poison;
                }

                let elem = self.ty(first).clone();
                self.set_known(*id, Type::Array(BoxedType::owned(elem)))
            }
            Node::Record { id, fields, .. } => {
                let mut typed_fields = Vec::with_capacity(fields.len());
                let mut poisoned = false;

                for (key, value) in fields {
                    let lex::Type::Ident(inner_name) = key.t else {
                        unreachable!()
                    };

                    match self.node(*value).known() {
                        Some(ty) => typed_fields.push(purple_garden_ir::ptype::Field {
                            name: inner_name,
                            ty: self.ty(ty).clone(),
                        }),
                        None => poisoned = true,
                    }
                }

                if poisoned {
                    return TcType::Poison;
                }

                self.set_known(*id, Type::Record(typed_fields.into()))
            }
            Node::Atom { id, raw } => {
                let t = purple_garden_frontend::type_from_atom_token_type(&raw.t);
                self.set_known(*id, t)
            }
            Node::Ident { id, name } => {
                let lex::Token {
                    t: lex::Type::Ident(inner_name),
                    ..
                } = name
                else {
                    unreachable!()
                };

                let Some(binding) = self.env_get(inner_name) else {
                    self.report(Diagnostic::at_token(
                        format!("binding `{inner_name}` not found"),
                        name,
                    ));
                    return TcType::Poison;
                };

                self.map.bind(*id, binding);
                TcType::Known(*id)
            }
            Node::Bin { id, op, lhs, rhs } => {
                let lhs = self.node(*lhs);
                let rhs = self.node(*rhs);
                let (Some(lhs), Some(rhs)) = (lhs.known(), rhs.known()) else {
                    return TcType::Poison;
                };
                match Self::fuse(op, self.ty(lhs), self.ty(rhs)) {
                    Ok(t) => self.set_known(*id, t),
                    Err(err) => {
                        self.report(err);
                        TcType::Poison
                    }
                }
            }
            Node::Unary { id, op, rhs } => {
                let Some(inner) = self.node(*rhs).known() else {
                    return TcType::Poison;
                };
                let t = match (&op.t, self.ty(inner)) {
                    (lex::Type::Plus | lex::Type::Minus, Type::Int) => Type::Int,
                    (lex::Type::Plus | lex::Type::Minus, Type::Double) => Type::Double,
                    (_, inner) => {
                        let err = Diagnostic::at_token(
                            format!(
                                "Unary {:?} requires Int or Double, got {}",
                                op.t.as_str(),
                                inner
                            ),
                            op,
                        );
                        self.report(err);
                        return TcType::Poison;
                    }
                };
                self.set_known(*id, t)
            }
            Node::Let { id, name, rhs, .. } => {
                let lex::Token {
                    t: lex::Type::Ident(inner_name),
                    ..
                } = name
                else {
                    unreachable!()
                };

                let Some(inner) = self.node(*rhs).known() else {
                    return TcType::Poison;
                };
                let binding = self
                    .map
                    .ref_of(inner)
                    .expect("known ids always have a type");
                self.env_insert(inner_name, binding);
                self.map.bind(*id, binding);
                TcType::Known(*id)
            }
            Node::Fn {
                id,
                name,
                args,
                return_type,
                body,
                ..
            } => {
                let lex::Token {
                    t: lex::Type::Ident(inner_name),
                    ..
                } = name
                else {
                    unreachable!()
                };

                if self.functions.contains_key(inner_name) {
                    self.report(Diagnostic::at_token(
                        format!("`{inner_name}` is already defined"),
                        name,
                    ));
                    return TcType::Poison;
                }

                let prev_env = std::mem::take(&mut self.env);
                self.env.push(HashMap::new());
                let mut typed_arguments = Vec::with_capacity(args.len());
                for (arg_name, arg_type) in args {
                    let lex::Token {
                        t: lex::Type::Ident(inner_name),
                        ..
                    } = arg_name
                    else {
                        unreachable!()
                    };
                    let inner_name = *inner_name;

                    let t = purple_garden_frontend::type_from_type_expr(self.ast, *arg_type);
                    let binding = self.map.alloc(t.clone());
                    self.env_insert(inner_name, binding);
                    typed_arguments.push((inner_name, t));
                }

                let ret: Type<'t> =
                    purple_garden_frontend::type_from_type_expr(self.ast, *return_type);
                let f_type = FunctionType {
                    args: typed_arguments,
                    ret: ret.clone(),
                    with_slots: false,
                };
                purple_garden_shared::trace!(
                    "[ir::typecheck::Typechecker::node][{}]: {}",
                    inner_name,
                    f_type
                );
                self.functions.insert(inner_name, f_type);

                if let Some(computed_ret) = self.block_type(body).known()
                    && self.ty(computed_ret) != &ret
                {
                    let err = Diagnostic::at_token(
                        format!(
                            "`{inner_name}` should return {ret}, but returns {}",
                            self.ty(computed_ret)
                        ),
                        self.ast.type_token(*return_type),
                    );
                    self.report(err);
                }

                self.env = prev_env;
                self.set_known(*id, ret)
            }
            Node::Cast { id, lhs, rhs, src } => {
                let rhs = purple_garden_frontend::type_from_type_expr(self.ast, *rhs);
                let Some(lhs) = self.node(*lhs).known() else {
                    return TcType::Poison;
                };
                match self.cast(src, lhs, rhs) {
                    Some(t) => self.set_known(*id, t),
                    None => TcType::Poison,
                }
            }
            Node::Field { id, target, name } => {
                let Some(target) = self.node(*target).known() else {
                    return TcType::Poison;
                };
                let lex::Type::Ident(idx_path_end) = name.t else {
                    unreachable!();
                };

                let Type::Record(fields) = self.ty(target) else {
                    let err = Diagnostic::at_token(
                        format!("{} can not be indexed in this way", self.ty(target)),
                        name,
                    );
                    self.report(err);
                    return TcType::Poison;
                };

                // PERF: this record path lookup should mabye be a map
                let Some(field) = fields
                    .as_slice()
                    .iter()
                    .find(|field| field.name == idx_path_end)
                else {
                    let err = Diagnostic::at_token(
                        format!(
                            "{} does not have a field called {idx_path_end}",
                            self.ty(target)
                        ),
                        name,
                    );
                    self.report(err);
                    return TcType::Poison;
                };

                let t = field.ty.clone();
                self.set_known(*id, t)
            }
            Node::Call { id, target, args } => match self.ast.node(*target) {
                Node::Field { target, name, .. } => self.resolve_pkg_call(*id, *target, name, args),
                Node::Ident { name, .. } => self.resolve_call(*id, name, args),
                _ => unreachable!(),
            },
            Node::Match { id, cases, default } => {
                // all branches MUST resolve to the same type :)
                let mut branch_types: Vec<Option<(&Token, usize)>> = vec![None; cases.len()];

                for (i, ((condition_token, condition), body)) in cases.iter().enumerate() {
                    if let Some(condition_type) = self.node(*condition).known()
                        && self.ty(condition_type) != &Type::Bool
                    {
                        let err = Diagnostic::at_token(
                            format!(
                                "Match conditions must be Bool, got {} instead",
                                self.ty(condition_type)
                            ),
                            condition_token,
                        );
                        self.report(err);
                    }

                    if let Some(branch_return_type) = self.block_type(body).known() {
                        branch_types[i] = Some((condition_token, branch_return_type));
                    }
                }

                // we simply use the default branches type as the canonical type of the match, its
                // the easiest way to deal with this
                let Some(first_type) = self.block_type(&default.1).known() else {
                    return TcType::Poison;
                };

                for cur in &branch_types {
                    let Some((tok, ty)) = cur else { continue };

                    if self.ty(*ty) != self.ty(first_type) {
                        let err = Diagnostic::at_token(
                            format!(
                                "Match cases must resolve to the same type, but got {} and {}",
                                self.ty(first_type),
                                self.ty(*ty)
                            ),
                            tok,
                        );
                        self.report(err);
                    }
                }

                self.alias(*id, first_type)
            }
            Node::Import { id, pkgs, src } => {
                if pkgs.is_empty() {
                    self.report(Diagnostic::at_token(
                        "Import without any paths to import is considered invalid",
                        src,
                    ));
                    return self.alias(*id, self.void);
                }

                for pkg_tok in pkgs {
                    let lex::Type::S(pkg_name) = pkg_tok.t else {
                        unreachable!();
                    };

                    if self.packages.contains_key(pkg_name) {
                        continue;
                    }

                    let Some(pkg) = self.resolve_pkg(pkg_name) else {
                        self.report(Diagnostic::at_token(
                            format!("Wasnt able to find a package named `{pkg_name}`"),
                            pkg_tok,
                        ));
                        continue;
                    };

                    purple_garden_shared::trace!(
                        "[ir::typecheck::Typechecker::node] resolved pkg `{}`",
                        pkg.name
                    );

                    self.register_pkg(pkg);
                }

                self.alias(*id, self.void)
            }
            Node::Extern { id, .. } => {
                self.register_extern(node_id);
                self.alias(*id, self.void)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purple_garden_frontend::{lex::Lexer, parser::Parser};

    fn parse(source: &[u8]) -> Ast<'_> {
        Parser::new(Lexer::new(source)).parse().unwrap()
    }

    fn type_of<'t>(ast: &Ast<'t>, out: &TypecheckOutput<'t>, node: NodeId) -> Option<Type<'t>> {
        out.types.get(ast.value_id(node)).cloned()
    }

    #[test]
    fn output_outlives_the_ast_it_was_checked_against() {
        fn check(source: &[u8]) -> TypecheckOutput<'_> {
            let ast = parse(source);
            Typechecker::new(&ast).check()
        }

        let out = check(br#"fn wrap(value:Int) Record<value: Int> { { value: value } }"#);
        let wrap = out.functions.get("wrap").expect("fn is registered");

        assert_eq!(wrap.args, vec![("value", Type::Int)]);
        assert_eq!(wrap.ret, Type::record(vec![("value", Type::Int)]));
    }

    #[test]
    fn record_field_access_resolves_field_type() {
        let ast = parse(br#"{ name: "teo" age: 23 }.name"#);
        let out = Typechecker::new(&ast).check();

        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(type_of(&ast, &out, ast.roots[0]), Some(Type::Str));
    }

    #[test]
    fn nested_record_field_access_resolves_inner_field_type() {
        let ast = parse(br#"{ name: "teo" job: { title: "dev" since: 2024 } }.job.since"#);
        let out = Typechecker::new(&ast).check();

        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(type_of(&ast, &out, ast.roots[0]), Some(Type::Int));
    }

    #[test]
    fn unknown_record_field_reports_error() {
        let ast = parse(br#"{ name: "teo" age: 23 }.missing"#);
        let out = Typechecker::new(&ast).check();

        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(
            out.diagnostics[0].message,
            "Record<name: Str age: Int> does not have a field called missing"
        );
    }

    #[test]
    fn field_call_with_non_package_target_reports_error() {
        let ast = parse(br#"{ job: { run: "nope" } }.job.run()"#);
        let out = Typechecker::new(&ast).check();

        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(
            out.diagnostics[0].message,
            "only package functions can be called through field syntax"
        );
    }

    #[test]
    fn homogeneous_array_resolves_element_type() {
        let ast = parse(b"[1 2 3]");
        let out = Typechecker::new(&ast).check();

        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(
            type_of(&ast, &out, ast.roots[0]),
            Some(Type::Array(BoxedType::owned(Type::Int)))
        );
    }

    #[test]
    fn mixed_array_members_report_error_at_offending_member() {
        let ast = parse(br#"[1 "two" 3]"#);
        let out = Typechecker::new(&ast).check();

        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(
            out.diagnostics[0].message,
            "Array members must all have the same type, expected Int but got Str"
        );
        assert_eq!(out.diagnostics[0].primary.span, Span::new(4, 3));
        assert_eq!(
            out.diagnostics[0].primary.message.as_deref(),
            Some("this member is of type Str")
        );
        assert_eq!(
            out.diagnostics[0].notes,
            ["the array element type was inferred as Int from the first member"]
        );
    }

    #[test]
    fn empty_array_reports_inference_error() {
        let ast = parse(b"[]");
        let out = Typechecker::new(&ast).check();

        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(
            out.diagnostics[0].message,
            "Can not infer the element type of an empty array"
        );
        assert_eq!(out.diagnostics[0].primary.span, Span::new(0, 1));
        assert_eq!(
            out.diagnostics[0].primary.message.as_deref(),
            Some("empty array")
        );
        assert_eq!(type_of(&ast, &out, ast.roots[0]), None);
    }

    #[test]
    fn array_typecheck_does_not_report_first_member_errors_twice() {
        let ast = parse(br#"[{ jobs: ["opfer"] } { jobs: [opfer] }]"#);
        let out = Typechecker::new(&ast).check();

        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].message, "binding `opfer` not found");
    }

    #[test]
    fn nested_record_literal_stores_inline_fields() {
        use purple_garden_frontend::lower::Lower;
        use purple_garden_ir::{Id, Instr};

        let ast = parse(br#"{ name: "teo" age: 23 job: { name: "dev" since: 2024 } }"#);
        let typecheck = Typechecker::new(&ast).check();
        assert!(
            typecheck.diagnostics.is_empty(),
            "{:?}",
            typecheck.diagnostics
        );
        let funcs = Lower::new().ir_from_types(&ast, typecheck.types).unwrap();
        let instructions = &funcs[0].blocks[0].instructions;

        let allocs = instructions
            .iter()
            .filter(|instr| matches!(instr, Instr::Alloc { .. }))
            .count();
        let stores = instructions
            .iter()
            .filter_map(|instr| match instr {
                Instr::Store { base, offset, .. } => Some((*base, *offset)),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(allocs, 1);
        assert_eq!(
            stores,
            vec![(Id(0), 0), (Id(0), 8), (Id(0), 16), (Id(0), 24)]
        );
    }

    #[test]
    fn array_literal_stores_length_and_members_inline() {
        use purple_garden_frontend::lower::Lower;
        use purple_garden_ir::{Const, Id, Instr};

        let ast = parse(b"[1 2]");
        let typecheck = Typechecker::new(&ast).check();
        assert!(
            typecheck.diagnostics.is_empty(),
            "{:?}",
            typecheck.diagnostics
        );
        let funcs = Lower::new().ir_from_types(&ast, typecheck.types).unwrap();
        let instructions = &funcs[0].blocks[0].instructions;

        let alloc = instructions
            .iter()
            .find_map(|instr| match instr {
                Instr::Alloc { dst, layout, .. } => Some((dst.id, dst.ty.clone(), *layout)),
                _ => None,
            })
            .expect("array literal should allocate");
        let consts = instructions
            .iter()
            .filter_map(|instr| match instr {
                Instr::LoadConst { dst, value, .. } => Some((dst.id, value.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();
        let stores = instructions
            .iter()
            .filter_map(|instr| match instr {
                Instr::Store {
                    src, base, offset, ..
                } => Some((*src, *base, *offset)),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(alloc.0, Id(0));
        assert_eq!(alloc.1, Type::Array(BoxedType::owned(Type::Int)));
        assert_eq!(alloc.2.size(), 24);
        assert_eq!(alloc.2.align(), 8);
        assert_eq!(
            consts,
            vec![
                (Id(1), Const::Int(2)),
                (Id(2), Const::Int(1)),
                (Id(3), Const::Int(2))
            ]
        );
        assert_eq!(
            stores,
            vec![(Id(1), Id(0), 0), (Id(2), Id(0), 8), (Id(3), Id(0), 16)]
        );
    }

    #[test]
    fn array_literal_stores_record_members_inline() {
        use purple_garden_frontend::lower::Lower;
        use purple_garden_ir::{Id, Instr};

        let ast = parse(b"[{ x: 1 y: 2 } { x: 3 y: 4 }]");
        let typecheck = Typechecker::new(&ast).check();
        assert!(
            typecheck.diagnostics.is_empty(),
            "{:?}",
            typecheck.diagnostics
        );
        let funcs = Lower::new().ir_from_types(&ast, typecheck.types).unwrap();
        let instructions = &funcs[0].blocks[0].instructions;

        let allocs = instructions
            .iter()
            .filter_map(|instr| match instr {
                Instr::Alloc { dst, layout, .. } => Some((dst.id, dst.ty.clone(), *layout)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let stores = instructions
            .iter()
            .filter_map(|instr| match instr {
                Instr::Store { base, offset, .. } => Some((*base, *offset)),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(allocs.len(), 1);
        assert_eq!(allocs[0].0, Id(0));
        assert_eq!(
            allocs[0].1,
            Type::Array(BoxedType::owned(Type::record(vec![
                ("x", Type::Int),
                ("y", Type::Int),
            ])))
        );
        assert_eq!(allocs[0].2.size(), 40);
        assert_eq!(allocs[0].2.align(), 8);
        assert_eq!(
            stores,
            vec![
                (Id(0), 0),
                (Id(0), 8),
                (Id(0), 16),
                (Id(0), 24),
                (Id(0), 32)
            ]
        );
    }

    #[test]
    fn binary_int_arithmetic_produces_int() {
        let ast = parse(b"1 + 2");
        let out = Typechecker::new(&ast).check();
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(type_of(&ast, &out, ast.roots[0]), Some(Type::Int));
    }

    #[test]
    fn binary_comparison_produces_bool() {
        let ast = parse(b"1 < 2");
        let out = Typechecker::new(&ast).check();
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(type_of(&ast, &out, ast.roots[0]), Some(Type::Bool));
    }

    #[test]
    fn binary_mismatched_operands_report_error() {
        let ast = parse(br#"1 + "s""#);
        let out = Typechecker::new(&ast).check();
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(
            out.diagnostics[0].message,
            "Incompatible types Int and Str for \"+\""
        );
    }

    #[test]
    fn fn_return_type_mismatch_reports_error() {
        let ast = parse(b"fn one() Str { 1 }");
        let out = Typechecker::new(&ast).check();
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(
            out.diagnostics[0].message,
            "`one` should return Str, but returns Int"
        );
    }

    #[test]
    fn fn_redeclaration_reports_error() {
        let ast = parse(b"fn dup() Int { 1 } fn dup() Int { 2 }");
        let out = Typechecker::new(&ast).check();
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].message, "`dup` is already defined");
    }

    #[test]
    fn match_non_bool_condition_reports_error() {
        let ast = parse(b"match { 1 { 2 } { 3 } }");
        let out = Typechecker::new(&ast).check();
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(
            out.diagnostics[0].message,
            "Match conditions must be Bool, got Int instead"
        );
    }

    #[test]
    fn match_branches_type_mismatch_reports_error() {
        let ast = parse(br#"match { 1 == 1 { "yes" } { 0 } }"#);
        let out = Typechecker::new(&ast).check();
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(
            out.diagnostics[0].message,
            "Match cases must resolve to the same type, but got Int and Str"
        );
    }

    #[test]
    fn cast_illegal_reports_error() {
        let ast = parse(br#""foo" as Int"#);
        let out = Typechecker::new(&ast).check();
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].message, "Can not cast Str to Int");
    }

    #[test]
    fn call_wrong_arity_reports_error() {
        let ast = parse(b"fn add(a:Int b:Int) Int { a + b } add(1)");
        let out = Typechecker::new(&ast).check();
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(
            out.diagnostics[0].message,
            "`add` requires 2 arguments, got 1"
        );
    }

    #[test]
    fn call_wrong_arg_type_reports_error() {
        let ast = parse(br#"fn add(a:Int b:Int) Int { a + b } add(1 "s")"#);
        let out = Typechecker::new(&ast).check();
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(
            out.diagnostics[0].message,
            "`add` expected b:Int, got b:Str instead"
        );
    }

    fn check<'s>(source: &'s [u8]) -> (Ast<'s>, TypecheckOutput<'s>) {
        let ast = parse(source);
        let out = Typechecker::new(&ast).check();
        (ast, out)
    }

    fn root_type<'t>(ast: &Ast<'t>, out: &TypecheckOutput<'t>, root: usize) -> Option<Type<'t>> {
        type_of(ast, out, ast.roots[root])
    }

    fn messages<'o>(out: &'o TypecheckOutput<'_>) -> Vec<&'o str> {
        out.diagnostics.iter().map(|d| d.message.as_str()).collect()
    }

    fn opt(ty: Type<'static>) -> Type<'static> {
        Type::Option(BoxedType::owned(ty))
    }

    #[test]
    fn declarations_are_typed_void_or_their_return_type() {
        let (ast, out) =
            check(b"import \"math\"\nfn f(a:Int) Int { a }\nextern \"ffi\" { fn g(a:Int) Bool }");
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(root_type(&ast, &out, 0), Some(Type::Void));
        assert_eq!(root_type(&ast, &out, 1), Some(Type::Int));
        assert_eq!(root_type(&ast, &out, 2), Some(Type::Void));
    }

    #[test]
    fn type_map_covers_every_value_id_and_ends_with_the_void_slot() {
        let (ast, out) = check(b"let x = { a: [1 2] b: -1 }\nx.a\nfn f() Int { 1 }\nf()");
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(out.types.slots(), ast.values + 1);
        assert_eq!(out.types.get(ast.values), Some(&Type::Void));
        for root in 0..ast.roots.len() {
            assert!(
                root_type(&ast, &out, root).is_some(),
                "root {root} is typed"
            );
        }
    }

    #[test]
    fn let_types_the_binding_and_later_reads() {
        let (ast, out) = check(b"let x = 1.5\nx");
        assert!(out.diagnostics.is_empty());
        assert_eq!(root_type(&ast, &out, 0), Some(Type::Double));
        assert_eq!(root_type(&ast, &out, 1), Some(Type::Double));
    }

    #[test]
    fn unknown_binding_reports_error_and_poisons() {
        let (ast, out) = check(b"y");
        assert_eq!(messages(&out), vec!["binding `y` not found"]);
        assert_eq!(root_type(&ast, &out, 0), None);
    }

    #[test]
    fn unary_minus_types_int_and_double() {
        let (ast, out) = check(b"-1\n-1.5");
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(root_type(&ast, &out, 0), Some(Type::Int));
        assert_eq!(root_type(&ast, &out, 1), Some(Type::Double));
    }

    #[test]
    fn unary_on_str_reports_error() {
        let (ast, out) = check(br#"-"s""#);
        assert_eq!(
            messages(&out),
            vec![r#"Unary "-" requires Int or Double, got Str"#]
        );
        assert_eq!(root_type(&ast, &out, 0), None);
    }

    #[test]
    fn cast_int_to_double_types_double() {
        let (ast, out) = check(b"1 as Double");
        assert!(out.diagnostics.is_empty());
        assert_eq!(root_type(&ast, &out, 0), Some(Type::Double));
    }

    #[test]
    fn redundant_cast_reports_but_keeps_the_type() {
        let (ast, out) = check(b"1 as Int");
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(root_type(&ast, &out, 0), Some(Type::Int));
    }

    #[test]
    fn match_takes_the_default_branch_type() {
        let (ast, out) = check(b"match { 1 == 1 { 2 } { 3 } }");
        assert!(out.diagnostics.is_empty());
        assert_eq!(root_type(&ast, &out, 0), Some(Type::Int));
    }

    #[test]
    fn default_only_match_takes_the_default_branch_type() {
        let (ast, out) = check(b"let x = match { { 3 } }\nx + 1");
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(root_type(&ast, &out, 0), Some(Type::Int));
        assert_eq!(root_type(&ast, &out, 1), Some(Type::Int));
    }

    #[test]
    fn fn_parameters_are_scoped_to_the_body() {
        let (ast, out) = check(b"fn f(a:Int) Int { a }\na");
        assert_eq!(messages(&out), vec!["binding `a` not found"]);
        assert_eq!(root_type(&ast, &out, 0), Some(Type::Int));
    }

    #[test]
    fn fn_body_let_chain_typechecks_against_declared_return() {
        let (ast, out) = check(b"fn f(n:Int) Int { let a = n + 1\nlet b = a * 2\nb }\nf(1)");
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(root_type(&ast, &out, 1), Some(Type::Int));
    }

    #[test]
    fn fn_body_returning_void_reports_mismatch() {
        let (_, out) = check(b"fn f() Int { }");
        assert_eq!(
            messages(&out),
            vec!["`f` should return Int, but returns Void"]
        );
    }

    #[test]
    fn array_with_poisoned_first_member_reports_once() {
        let (ast, out) = check(b"[nope 1]");
        assert_eq!(messages(&out), vec!["binding `nope` not found"]);
        assert_eq!(root_type(&ast, &out, 0), None);
    }

    #[test]
    fn array_with_poisoned_later_member_reports_once() {
        let (ast, out) = check(b"[1 nope]");
        assert_eq!(messages(&out), vec!["binding `nope` not found"]);
        assert_eq!(root_type(&ast, &out, 0), None);
    }

    #[test]
    fn record_with_poisoned_field_reports_once() {
        let (ast, out) = check(b"{ a: nope b: 1 }");
        assert_eq!(messages(&out), vec!["binding `nope` not found"]);
        assert_eq!(root_type(&ast, &out, 0), None);
    }

    #[test]
    fn field_access_on_non_record_reports_error() {
        let (ast, out) = check(b"1.x");
        assert_eq!(messages(&out), vec!["Int can not be indexed in this way"]);
        assert_eq!(root_type(&ast, &out, 0), None);
    }

    #[test]
    fn package_call_resolves_overload_return_type() {
        let (ast, out) = check(b"import \"math\"\nmath.abs(1)\nmath.abs(1.5)");
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(root_type(&ast, &out, 1), Some(Type::Int));
        assert_eq!(root_type(&ast, &out, 2), Some(Type::Double));
    }

    #[test]
    fn package_call_overload_miss_reports_error() {
        let (ast, out) = check(b"import \"math\"\nmath.abs(\"s\")");
        assert_eq!(out.diagnostics.len(), 1);
        assert!(
            out.diagnostics[0]
                .message
                .starts_with("no specialisation of `math.abs` accepts (Str)"),
            "{}",
            out.diagnostics[0].message
        );
        assert_eq!(root_type(&ast, &out, 1), None);
    }

    #[test]
    fn undefined_package_function_reports_error() {
        let (_, out) = check(b"import \"math\"\nmath.nope(1)");
        assert_eq!(
            messages(&out),
            vec!["Call to undefined function `math.nope`"]
        );
    }

    #[test]
    fn calling_an_unimported_package_reports_error() {
        let (_, out) = check(b"math.abs(1)");
        assert_eq!(messages(&out), vec!["Can't find package `math`"]);
    }

    #[test]
    fn import_of_unknown_package_reports_error() {
        let (_, out) = check(b"import \"nope\"");
        assert_eq!(
            messages(&out),
            vec!["Wasnt able to find a package named `nope`"]
        );
    }

    #[test]
    fn undefined_function_still_types_its_arguments() {
        let (_, out) = check(b"nope(x)");
        assert_eq!(
            messages(&out),
            vec!["binding `x` not found", "Call to undefined function `nope`"]
        );
    }

    #[test]
    fn poisoned_call_argument_is_not_diagnosed_twice() {
        let (_, out) = check(b"import \"math\"\nmath.abs(x)");
        assert_eq!(messages(&out), vec!["binding `x` not found"]);

        let (_, out) = check(b"fn f(a:Int) Int { a }\nf(x)");
        assert_eq!(messages(&out), vec!["binding `x` not found"]);
    }

    #[test]
    fn call_arg_mismatch_points_at_the_argument() {
        let (_, out) = check(b"fn f(a:Int) Int { a }\nf(\"s\")");
        assert_eq!(
            messages(&out),
            vec!["`f` expected a:Int, got a:Str instead"]
        );
        assert_eq!(out.diagnostics[0].primary.span, Span::new(25, 1));
        assert_eq!(
            out.diagnostics[0].primary.message.as_deref(),
            Some("this argument is of type Str")
        );
    }

    #[test]
    fn generic_call_substitutes_the_return_type() {
        let (ast, out) =
            check(b"import \"opt\"\nopt.some(1)\nopt.some(opt.some(\"s\"))\nopt.some([1 2])");
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        assert_eq!(root_type(&ast, &out, 1), Some(opt(Type::Int)));
        assert_eq!(root_type(&ast, &out, 2), Some(opt(opt(Type::Str))));
        assert_eq!(
            root_type(&ast, &out, 3),
            Some(opt(Type::Array(BoxedType::owned(Type::Int))))
        );
    }

    #[test]
    fn generic_result_flows_into_concrete_parameters() {
        let (_, out) = check(
            b"import \"opt\"\nfn f(a: Option<Int>) Int { 1 }\nf(opt.some(1))\nf(opt.some(\"s\"))",
        );
        assert_eq!(
            messages(&out),
            vec!["`f` expected a:Option<Int>, got a:Option<Str> instead"]
        );
    }

    #[test]
    fn record_arguments_compare_structurally() {
        let (ast, out) = check(b"fn first(r: Record<a: Int b: Str>) Int { r.a }\nfirst({ a: 1 b: \"s\" })\nfirst({ a: 1 b: 2 })");
        assert_eq!(
            messages(&out),
            vec!["`first` expected r:Record<a: Int b: Str>, got r:Record<a: Int b: Int> instead"]
        );
        assert_eq!(root_type(&ast, &out, 1), Some(Type::Int));
    }
}
