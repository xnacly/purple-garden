mod display;
mod typedefs;

use std::collections::HashMap;

use purple_garden_frontend::{
    ast::{Ast, Node, NodeId},
    diagnostic::{Diagnostic, Help, Span},
    lex::{self, Token},
};
use purple_garden_ir::ptype::Type;
use purple_garden_runtime::Pkg;
use purple_garden_std as pstd;

pub use typedefs::TypecheckOutput;
use typedefs::{FunctionType, TcType};

#[derive(Debug)]
pub struct Typechecker<'t> {
    ast: &'t Ast<'t>,
    /// Node id -> Type. Indexed by id; Node ids are dense from the parser.
    map: Vec<Option<Type<'t>>>,
    /// scope stack; innermost frame last; lookups walk from top to bottom
    env: Vec<HashMap<&'t str, Type<'t>>>,
    /// map a function name to its type(s)
    functions: HashMap<&'t str, FunctionType<'t>>,
    /// map a pkg name to a map of its public method names to overload groups
    /// (one entry per specialisation; >1 means a `specialises` group)
    packages: HashMap<&'t str, HashMap<&'t str, Vec<FunctionType<'t>>>>,
    pkg_cache: HashMap<&'t str, Option<&'t Pkg>>,
    libs: Vec<&'t Pkg>,
    stdlib: bool,
    diagnostics: Vec<Diagnostic>,
}

impl<'t> Typechecker<'t> {
    #[must_use]
    pub fn new(ast: &'t Ast<'t>) -> Self {
        let mut s = Self {
            ast,
            map: Vec::new(),
            env: Vec::new(),
            functions: HashMap::new(),
            packages: HashMap::new(),
            pkg_cache: HashMap::new(),
            libs: Vec::new(),
            stdlib: true,
            diagnostics: Vec::new(),
        };
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
        self.stdlib = stdlib;
        self
    }

    fn env_get(&self, k: &str) -> Option<&Type<'t>> {
        self.env.iter().rev().find_map(|frame| frame.get(k))
    }

    fn env_insert(&mut self, k: &'t str, v: Type<'t>) {
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
            .or_else(|| self.stdlib.then(|| pstd::resolve_pkg(query)).flatten());

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
            self.register_extern(node);
        }

        for &node in &self.ast.roots {
            self.node(node);
        }

        TypecheckOutput {
            types: self.map,
            diagnostics: self.diagnostics,
        }
    }

    fn report(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    fn set_type(&mut self, id: usize, t: Type<'t>) {
        if id >= self.map.len() {
            self.map.resize(id + 1, None);
        }
        self.map[id] = Some(t);
    }

    fn set_known(&mut self, id: usize, t: Type<'t>) -> TcType<'t> {
        self.set_type(id, t.clone());
        TcType::Known(t)
    }

    fn already_checked(&self, node: NodeId) -> Option<Type<'t>> {
        self.map
            .get(self.ast.value_id(node)?)
            .and_then(|o| o.as_ref())
            .cloned()
    }

    /// Type already assigned to `node`, by reference. Callers must have typed
    /// `node` first (via [`Self::node`]); used to read arg types for overload
    /// selection without cloning them out of the map.
    fn resolved_arg_ty(&self, node: NodeId) -> &Type<'t> {
        self.map[self.ast.value_id(node).expect("arg has a value id")]
            .as_ref()
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

    fn redundant_conversion_note(
        &self,
        args: &[NodeId],
        candidates: &[FunctionType<'t>],
    ) -> Option<String> {
        if args.len() != 1 {
            return None;
        }

        let provided_ty = self.resolved_arg_ty(args[0]);
        if !candidates
            .iter()
            .all(|c| c.args.len() == 1 && &c.ret == provided_ty)
        {
            return None;
        }

        let arg = self.node_label(args[0]).unwrap_or("the argument");
        Some(format!("`{arg}` is already {provided_ty}"))
    }

    fn redundant_cast_error(at: &lex::Token, ty: &Type<'t>) -> Diagnostic {
        Diagnostic::at_token(format!("Can not cast {ty} to {ty}"), at)
            .with_primary_message("unnecessary cast")
            .with_note(format!("the expression is already {ty}"))
            .with_help(Help::new("remove the cast"))
    }

    fn missing_package_error(&mut self, pkg_name: &'t str, pkg_tok: &lex::Token) -> Diagnostic {
        let mut err = Diagnostic::at_token(format!("Can't find package `{pkg_name}`"), pkg_tok)
            .with_primary_message("package used here");
        if self.resolve_pkg(pkg_name).is_some() {
            err = err
                .with_note(format!("package `{pkg_name}` exists but is not imported"))
                .with_help(
                    Help::new(format!("add `import \"{pkg_name}\"`"))
                        .with_replacement(Span::new(0, 0), format!("import \"{pkg_name}\"\n")),
                );
        }
        err
    }

    fn specialisation_miss_error(
        &self,
        pkg_name: &str,
        inner_name: &str,
        name: &lex::Token,
        args: &[NodeId],
        candidates: &[FunctionType<'t>],
    ) -> Diagnostic {
        fn sig<'a, 'b: 'a>(types: impl Iterator<Item = &'a Type<'b>>) -> String {
            types
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        }

        let provided = || args.iter().map(|&a| self.resolved_arg_ty(a));
        let avail = candidates
            .iter()
            .map(|c| sig(c.args.iter().map(|(_, t)| t)))
            .collect::<Vec<_>>()
            .join(" | ");
        let mut err = Diagnostic::at_token(
            format!(
                "no specialisation of `{pkg_name}.{inner_name}` accepts ({}); available: {avail}",
                sig(provided())
            ),
            name,
        );
        if let Some(note) = self.redundant_conversion_note(args, candidates) {
            err = err
                .with_note(note)
                .with_help(Help::new(format!("remove `{pkg_name}.{inner_name}`")));
        }
        err
    }

    fn common_return(candidates: &[FunctionType<'t>]) -> Option<Type<'t>> {
        let first = candidates.first()?.ret.clone();
        candidates.iter().all(|c| c.ret == first).then_some(first)
    }

    fn fuse(&mut self, op: &lex::Token, lhs: &Type<'t>, rhs: &Type<'t>) -> TcType<'t> {
        let ty = match op.t {
            // arithmetics
            lex::Type::Plus | lex::Type::Minus | lex::Type::Asteriks | lex::Type::Slash => {
                match (lhs, rhs) {
                    (Type::Int, Type::Int) => Type::Int,
                    (Type::Double, Type::Double) => Type::Double,
                    (_, _) if lhs == rhs => {
                        self.report(Diagnostic::at_token(
                            format!(
                                "Unsupported type {} for {:?}, want Int or Double",
                                lhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                        return TcType::Poison;
                    }
                    (_, _) => {
                        self.report(Diagnostic::at_token(
                            format!(
                                "Incompatible types {} and {} for {:?}",
                                lhs,
                                rhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                        return TcType::Poison;
                    }
                }
            }
            lex::Type::Percent => match (lhs, rhs) {
                (Type::Int, Type::Int) => Type::Int,
                (_, _) if lhs == rhs => {
                    self.report(Diagnostic::at_token(
                        format!("Unsupported type {} for {:?}, want Int", lhs, op.t.as_str()),
                        op,
                    ));
                    return TcType::Poison;
                }
                (_, _) => {
                    self.report(Diagnostic::at_token(
                        format!(
                            "Incompatible types {} and {} for {:?}, want both sides Int",
                            lhs,
                            rhs,
                            op.t.as_str()
                        ),
                        op,
                    ));
                    return TcType::Poison;
                }
            },
            lex::Type::DoubleEqual | lex::Type::NotEqual => {
                match (lhs, rhs) {
                    (Type::Int, Type::Int) | (Type::Bool, Type::Bool) => {}
                    (_, _) if lhs == rhs => {
                        self.report(Diagnostic::at_token(
                            format!(
                                "Unsupported type {} for {:?}, want Int or Bool",
                                lhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                        return TcType::Poison;
                    }
                    (_, _) => {
                        self.report(Diagnostic::at_token(
                            format!(
                                "Incompatible types {} and {} for {:?}, want both sides Int or both sides Bool",
                                lhs,
                                rhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                        return TcType::Poison;
                    }
                }
                Type::Bool
            }
            lex::Type::LessThan | lex::Type::GreaterThan => {
                match (lhs, rhs) {
                    (Type::Double, Type::Double) | (Type::Int, Type::Int) => {}
                    (_, _) if lhs == rhs => {
                        self.report(Diagnostic::at_token(
                            format!(
                                "Unsupported type {} for {:?}, want Int or Double for both sides",
                                lhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                        return TcType::Poison;
                    }
                    (_, _) => {
                        self.report(Diagnostic::at_token(
                            format!(
                                "Incompatible types {} and {} for {:?}",
                                lhs,
                                rhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                        return TcType::Poison;
                    }
                }
                Type::Bool
            }

            // lex::Type::Exclaim => todo!(),
            // lex::Type::Question => todo!(),
            _ => unreachable!(),
        };
        TcType::Known(ty)
    }

    fn cast(&mut self, at: &lex::Token, i: &Type<'t>, o: &Type<'t>) -> TcType<'t> {
        match (i, o) {
            (Type::Int, Type::Double) => TcType::Known(Type::Double),
            (Type::Double | Type::Bool, Type::Int) => TcType::Known(Type::Int),
            (Type::Int, Type::Bool) => TcType::Known(Type::Bool),
            (_, _) if i == o => {
                // This is still an error in PG, but the expression's type is
                // unambiguous. Keeping it known prevents downstream false
                // positives and makes `--types` more useful.
                self.report(Self::redundant_cast_error(at, i));
                TcType::Known(o.clone())
            }
            (_, _) => {
                self.report(Diagnostic::at_token(format!("Can not cast {i} to {o}"), at));
                TcType::Poison
            }
        }
    }

    fn block_type(&mut self, nodes: &[NodeId]) -> TcType<'t> {
        self.env.push(HashMap::new());
        let mut last_type = TcType::Known(Type::Void);
        for &node in nodes {
            last_type = self.node(node);
        }
        self.env.pop();
        last_type
    }

    fn node(&mut self, node_id: NodeId) -> TcType<'t> {
        let node = self.ast.node(node_id);
        if let Some(t) = self.already_checked(node_id) {
            return TcType::Known(t);
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

                let (first_type, mut poisoned) = match self.node(*first_member) {
                    TcType::Known(ty) => (ty, false),
                    TcType::Poison => (Type::Void, true),
                };

                for member in members.iter().skip(1) {
                    match self.node(*member) {
                        TcType::Known(ty) => {
                            if ty != first_type {
                                let span = self
                                    .ast
                                    .span(*member)
                                    .unwrap_or_else(|| Span::from_token(src));
                                self.report(
                                    Diagnostic::new(
                                        format!(
                                            "Array members must all have the same type, expected {first_type} but got {ty}"
                                        ),
                                        span,
                                    )
                                    .with_primary_message(format!("this member is of type {ty}"))
                                    .with_note(format!(
                                        "the array element type was inferred as {first_type} from the first member"
                                    )),
                                );

                                return TcType::Poison;
                            }
                        }
                        TcType::Poison => {
                            poisoned = true;
                        }
                    }
                }

                if poisoned {
                    return TcType::Poison;
                }

                self.set_known(*id, Type::Array(Box::new(first_type)))
            }
            Node::Record { id, fields, .. } => {
                let mut typed_fields = Vec::with_capacity(fields.len());
                let mut poisoned = false;

                for (key, value) in fields {
                    let lex::Type::Ident(inner_name) = key.t else {
                        unreachable!()
                    };

                    match self.node(*value) {
                        TcType::Known(ty) => {
                            typed_fields.push(purple_garden_ir::ptype::Field {
                                name: inner_name,
                                ty,
                            });
                        }
                        TcType::Poison => {
                            poisoned = true;
                        }
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

                let Some(t) = self.env_get(inner_name).cloned() else {
                    self.report(Diagnostic::at_token(
                        format!("binding `{inner_name}` not found"),
                        name,
                    ));
                    return TcType::Poison;
                };

                self.set_known(*id, t)
            }
            Node::Bin { id, op, lhs, rhs } => {
                let lhs = self.node(*lhs);
                let rhs = self.node(*rhs);
                let (Some(lhs), Some(rhs)) = (lhs.as_known(), rhs.as_known()) else {
                    return TcType::Poison;
                };
                let res = self.fuse(op, lhs, rhs);
                if let Some(ty) = res.clone().known() {
                    self.set_type(*id, ty);
                }
                res
            }
            Node::Unary { id, op, rhs } => {
                let inner = self.node(*rhs);
                let Some(inner) = inner.as_known() else {
                    return TcType::Poison;
                };
                let t = match (&op.t, inner) {
                    (lex::Type::Plus | lex::Type::Minus, Type::Int) => Type::Int,
                    (lex::Type::Plus | lex::Type::Minus, Type::Double) => Type::Double,
                    _ => {
                        self.report(Diagnostic::at_token(
                            format!(
                                "Unary {:?} requires Int or Double, got {}",
                                op.t.as_str(),
                                inner
                            ),
                            op,
                        ));
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

                let inner = self.node(*rhs);
                let Some(inner) = inner.known() else {
                    return TcType::Poison;
                };
                self.set_type(*id, inner.clone());
                self.env_insert(inner_name, inner.clone());
                TcType::Known(inner)
            }
            Node::Fn {
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
                    self.env_insert(inner_name, t.clone());
                    typed_arguments.push((inner_name, t));
                }

                let ret: Type<'t> =
                    purple_garden_frontend::type_from_type_expr(self.ast, *return_type);
                let f_type = FunctionType {
                    args: typed_arguments,
                    ret: ret.clone(),
                };
                self.functions.insert(inner_name, f_type.clone());

                let computed_ret = self.block_type(body);
                if let Some(computed_ret) = computed_ret.as_known() {
                    if &ret != computed_ret {
                        self.report(Diagnostic::at_token(
                            format!(
                                "`{inner_name}` should return {ret}, but returns {computed_ret}"
                            ),
                            self.ast.type_token(*return_type),
                        ));
                    }
                }

                self.env = prev_env;
                purple_garden_shared::trace!(
                    "[ir::typecheck::Typechecker::node][{}]: {}",
                    inner_name,
                    f_type
                );
                TcType::Known(ret)
            }
            Node::Cast { id, lhs, rhs, src } => {
                let rhs = purple_garden_frontend::type_from_type_expr(self.ast, *rhs);
                let lhs = self.node(*lhs);
                let Some(lhs) = lhs.as_known() else {
                    return TcType::Poison;
                };
                let cast = self.cast(src, lhs, &rhs);
                if let Some(ty) = cast.clone().known() {
                    self.set_type(*id, ty);
                }
                cast
            }
            Node::Field { id, target, name } => {
                let target_type = self.node(*target);

                match target_type {
                    TcType::Known(ref target @ Type::Record(ref fields)) => {
                        let lex::Type::Ident(idx_path_end) = name.t else {
                            unreachable!();
                        };

                        // PERF: this record path lookup should mabye be a map
                        let Some(field) = fields
                            .as_slice()
                            .iter()
                            .find(|field| field.name == idx_path_end)
                        else {
                            self.report(Diagnostic::at_token(
                                format!("{target} does not have a field called {idx_path_end}"),
                                name,
                            ));
                            return TcType::Poison;
                        };

                        self.set_type(*id, field.ty.clone());
                        TcType::Known(field.ty.clone())
                    }
                    TcType::Known(t) => {
                        self.report(Diagnostic::at_token(
                            format!("{t} can not be indexed in this way"),
                            name,
                        ));
                        TcType::Poison
                    }
                    _ => TcType::Poison,
                }
            }
            Node::Call { id, target, args } => {
                let (tok, inner_name, fun) = match self.ast.node(*target) {
                    Node::Field { target, name, .. } => {
                        let Node::Ident { name: pkg_tok, .. } = self.ast.node(*target) else {
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

                        // Type args up front: overload selection reads them back
                        // by reference, and `node` memoises so the single-candidate
                        // fall-through arity check below reuses the results. Doing
                        // this before borrowing `candidates` lets us hold that
                        // borrow instead of cloning the whole group.
                        let mut args_poisoned = false;
                        for &arg in args {
                            if self.node(arg).as_known().is_none() {
                                args_poisoned = true;
                            }
                        }

                        let Some(pkg) = self.packages.get(pkg_name) else {
                            let err = self.missing_package_error(pkg_name, pkg_tok);
                            self.report(err);
                            return TcType::Poison;
                        };

                        let Some(candidates) = pkg.get(inner_name).cloned() else {
                            self.report(Diagnostic::at_token(
                                format!("Call to undefined function `{pkg_name}.{inner_name}`"),
                                name,
                            ));
                            return TcType::Poison;
                        };

                        // A `specialises` group (>1 candidate) dispatches on the
                        // provided arg types. A single-candidate name falls
                        // through to the shared arity/per-arg checks below, which
                        // produce precise per-argument diagnostics.
                        if candidates.len() > 1 {
                            // If argument expressions were poisoned, exact
                            // overload selection is impossible. A shared return
                            // type is still useful enough to recover with.
                            if args_poisoned {
                                if let Some(ret) = Self::common_return(&candidates) {
                                    return self.set_known(*id, ret);
                                }
                                return TcType::Poison;
                            }

                            let provided = || args.iter().map(|&a| self.resolved_arg_ty(a));

                            let Some(idx) = candidates.iter().position(|c| {
                                purple_garden_frontend::overload_matches(
                                    c.args.iter().map(|(_, t)| t),
                                    provided(),
                                )
                            }) else {
                                let err = self.specialisation_miss_error(
                                    pkg_name,
                                    inner_name,
                                    name,
                                    args,
                                    &candidates,
                                );
                                self.report(err);
                                // `strings.from(Str)` is invalid, but every
                                // variant returns `Str`, so callers can still
                                // typecheck against that result.
                                if let Some(ret) = Self::common_return(&candidates) {
                                    return self.set_known(*id, ret);
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
                            return self.set_known(*id, ret);
                        }

                        let mut s = String::from(*pkg_name);
                        s.push('.');
                        s.push_str(inner_name);
                        (name, s, candidates[0].clone())
                    }
                    Node::Ident { name, .. } => {
                        let lex::Token {
                            t: lex::Type::Ident(inner_name),
                            ..
                        } = name
                        else {
                            unreachable!();
                        };
                        let Some(fun) = self.functions.get(inner_name).cloned() else {
                            self.report(Diagnostic::at_token(
                                format!("Call to undefined function `{inner_name}`"),
                                name,
                            ));
                            return TcType::Poison;
                        };
                        (name, inner_name.to_string(), fun)
                    }
                    _ => unreachable!(),
                };

                if args.len() != fun.args.len() {
                    self.report(Diagnostic::at_token(
                        format!(
                            "`{}` requires {} arguments, got {}",
                            inner_name,
                            fun.args.len(),
                            args.len()
                        ),
                        tok,
                    ));
                    return self.set_known(*id, fun.ret);
                }

                self.set_type(*id, fun.ret.clone());

                for (i, provided_node) in args.iter().enumerate() {
                    let provided_type = self.node(*provided_node);
                    let Some(provided_type) = provided_type.as_known() else {
                        continue;
                    };
                    let expected_type = &fun.args[i].1;

                    if expected_type != provided_type {
                        self.report(Diagnostic::at_token(
                            format!(
                                "`{inner_name}` arg{i} expected {expected_type}, got {provided_type} instead",
                            ),
                            // TODO: extract this token from provided_node
                            tok,
                        ));
                    }
                }

                TcType::Known(fun.ret)
            }
            Node::Match { id, cases, default } => {
                // short circuit for empty matches
                if cases.is_empty() {
                    return TcType::Known(Type::Void);
                }

                let case_count = cases.len();

                // all branches MUST resolve to the same type :)
                let mut branch_types: Vec<Option<(&Token, Type<'t>)>> =
                    vec![const { None }; case_count];

                for (i, ((condition_token, condition), body)) in cases.iter().enumerate() {
                    if let Some(condition_type) = self.node(*condition).known() {
                        if condition_type != Type::Bool {
                            self.report(Diagnostic::at_token(
                                format!(
                                    "Match conditions must be Bool, got {condition_type} instead"
                                ),
                                condition_token,
                            ));
                        }
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

                    if ty != &first_type {
                        self.report(Diagnostic::at_token(
                            format!(
                                "Match cases must resolve to the same type, but got {first_type} and {ty}"
                            ),
                            *tok,
                        ));
                    }
                }

                self.set_known(*id, first_type)
            }
            Node::Import { pkgs, src, .. } => {
                if pkgs.is_empty() {
                    self.report(Diagnostic::at_token(
                        "Import without any paths to import is considered invalid",
                        src,
                    ));
                    return TcType::Known(Type::Void);
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

                TcType::Known(Type::Void)
            }
            Node::Extern { .. } => TcType::Known(Type::Void),
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
        ast.value_id(node)
            .and_then(|id| out.types.get(id))
            .cloned()
            .flatten()
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
            Some(Type::Array(Box::new(Type::Int)))
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
        assert_eq!(alloc.1, Type::Array(Box::new(Type::Int)));
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
            Type::Array(Box::new(Type::record(vec![
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
}
