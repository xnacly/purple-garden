use std::{collections::HashMap, fmt::Display};

use crate::{
    ast::{Ast, Node, NodeId},
    err::PgError,
    lex::{self, Token},
};
use purple_garden_ir::ptype::Type;
use purple_garden_runtime::Pkg;
use purple_garden_std as pstd;

#[derive(Debug, Clone)]
struct FunctionType<'t> {
    args: Vec<(&'t str, Type<'t>)>,
    ret: Type<'t>,
}

impl Display for FunctionType<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(")?;
        for (i, (name, t)) in self.args.iter().enumerate() {
            write!(f, "{name}: {t}")?;
            if i + 1 == self.args.len() {
                continue;
            }
            write!(f, " ")?;
        }
        write!(f, ") -> {}", self.ret)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Typechecker<'t> {
    ast: &'t Ast<'t>,
    /// Node id -> Type. Indexed by id; Node ids are dense from the parser.
    map: Vec<Option<Type<'t>>>,
    /// scope stack; innermost frame last; lookups walk from top to bottom
    env: Vec<HashMap<&'t str, Type<'t>>>,
    /// map a function name to its type(s)
    functions: HashMap<&'t str, FunctionType<'t>>,
    /// map a pkg name to a map of its methods and their types
    packages: HashMap<&'t str, HashMap<&'t str, FunctionType<'t>>>,
    pkg_cache: HashMap<&'t str, Option<&'t Pkg>>,
    libs: Vec<&'t Pkg>,
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
        };
        s.env.push(HashMap::new());
        s
    }

    #[must_use]
    pub fn with_libs(mut self, libs: Vec<&'t Pkg>) -> Self {
        self.libs = libs;
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
            .or_else(|| pstd::resolve_pkg(query));

        self.pkg_cache.insert(query, pkg);
        pkg
    }

    #[must_use]
    pub fn finalise(self) -> Vec<Option<Type<'t>>> {
        self.map
    }

    fn set_type(&mut self, id: usize, t: Type<'t>) {
        if id >= self.map.len() {
            self.map.resize(id + 1, None);
        }
        self.map[id] = Some(t);
    }

    fn already_checked(&self, node: NodeId) -> Option<Type<'t>> {
        self.map
            .get(self.ast.value_id(node)?)
            .and_then(|o| o.as_ref())
            .cloned()
    }

    fn fuse(op: &lex::Token, lhs: &Type<'t>, rhs: &Type<'t>) -> Result<Type<'t>, PgError> {
        Ok(match op.t {
            // arithmetics
            lex::Type::Plus | lex::Type::Minus | lex::Type::Asteriks | lex::Type::Slash => {
                match (lhs, rhs) {
                    (Type::Int, Type::Int) => Type::Int,
                    (Type::Double, Type::Double) => Type::Double,
                    (_, _) if lhs == rhs => {
                        return Err(PgError::with_msg(
                            format!(
                                "Unsupported type {} for {:?}, want Int or Double",
                                lhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                    }
                    (_, _) => {
                        return Err(PgError::with_msg(
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
                    return Err(PgError::with_msg(
                        format!("Unsupported type {} for {:?}, want Int", lhs, op.t.as_str()),
                        op,
                    ));
                }
                (_, _) => {
                    return Err(PgError::with_msg(
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
                        return Err(PgError::with_msg(
                            format!(
                                "Unsupported type {} for {:?}, want Int or Bool",
                                lhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                    }
                    (_, _) => {
                        return Err(PgError::with_msg(
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
                        return Err(PgError::with_msg(
                            format!(
                                "Unsupported type {} for {:?}, want Int or Double for both sides",
                                lhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                    }
                    (_, _) => {
                        return Err(PgError::with_msg(
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
        })
    }

    fn cast(at: &lex::Token, i: &Type<'t>, o: &Type<'t>) -> Result<Type<'t>, PgError> {
        Ok(match (i, o) {
            (Type::Int, Type::Double) => Type::Double,
            (Type::Double | Type::Bool, Type::Int) => Type::Int,
            (Type::Int, Type::Bool) => Type::Bool,
            (_, _) => {
                return Err(PgError::with_msg(format!("Can not cast {i} to {o}"), at));
            }
        })
    }

    fn block_type(&mut self, nodes: &[NodeId]) -> Result<Type<'t>, PgError> {
        self.env.push(HashMap::new());
        let mut last_type = Type::Void;
        for &node in nodes {
            last_type = self.node(node)?;
        }
        self.env.pop();
        Ok(last_type)
    }

    pub fn node(&mut self, node_id: NodeId) -> Result<Type<'t>, PgError> {
        let node = self.ast.node(node_id);
        if let Some(t) = self.already_checked(node_id) {
            return Ok(t);
        }

        Ok(match node {
            Node::Atom { id, raw } => {
                let t = crate::type_from_atom_token_type(&raw.t);
                self.set_type(*id, t.clone());
                t
            }
            Node::Ident { id, name } => {
                let lex::Token {
                    t: lex::Type::Ident(inner_name),
                    ..
                } = name
                else {
                    unreachable!()
                };

                let t = self
                    .env_get(inner_name)
                    .ok_or_else(|| {
                        PgError::with_msg(format!("binding `{inner_name}` not found"), name)
                    })?
                    .clone();

                self.set_type(*id, t.clone());
                t
            }
            Node::Bin { id, op, lhs, rhs } => {
                let lhs = self.node(*lhs)?;
                let rhs = self.node(*rhs)?;
                let res = Self::fuse(op, &lhs, &rhs)?;
                self.set_type(*id, res.clone());
                res
            }
            Node::Unary { id, op, rhs } => {
                let inner = self.node(*rhs)?;
                let t = match (&op.t, &inner) {
                    (lex::Type::Plus | lex::Type::Minus, Type::Int) => Type::Int,
                    (lex::Type::Plus | lex::Type::Minus, Type::Double) => Type::Double,
                    _ => {
                        return Err(PgError::with_msg(
                            format!(
                                "Unary {:?} requires Int or Double, got {}",
                                op.t.as_str(),
                                inner
                            ),
                            op,
                        ));
                    }
                };
                self.set_type(*id, t.clone());
                t
            }
            Node::Let { id, name, rhs } => {
                let inner = self.node(*rhs)?;
                self.set_type(*id, inner.clone());
                let lex::Token {
                    t: lex::Type::Ident(inner_name),
                    ..
                } = name
                else {
                    unreachable!()
                };

                self.env_insert(inner_name, inner.clone());
                inner
            }
            Node::Fn {
                name,
                args,
                return_type,
                body,
            } => {
                let lex::Token {
                    t: lex::Type::Ident(inner_name),
                    ..
                } = name
                else {
                    unreachable!()
                };

                if self.functions.contains_key(inner_name) {
                    return Err(PgError::with_msg(
                        format!("`{inner_name}` is already defined"),
                        name,
                    ));
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

                    let t = crate::type_from_type_expr(self.ast, *arg_type);
                    self.env_insert(inner_name, t.clone());
                    typed_arguments.push((inner_name, t));
                }

                let ret: Type<'t> = crate::type_from_type_expr(self.ast, *return_type);
                let f_type = FunctionType {
                    args: typed_arguments,
                    ret: ret.clone(),
                };
                self.functions.insert(inner_name, f_type.clone());

                let computed_ret = self.block_type(body)?;
                if ret != computed_ret {
                    return Err(PgError::with_msg(
                        format!("`{inner_name}` should return {ret}, but returns {computed_ret}"),
                        self.ast.type_token(*return_type),
                    ));
                }

                self.env = prev_env;
                purple_garden_shared::trace!(
                    "[ir::typecheck::Typechecker::node][{}]: {}",
                    inner_name,
                    f_type
                );
                ret
            }
            Node::Cast { id, lhs, rhs, src } => {
                let rhs = crate::type_from_type_expr(self.ast, *rhs);
                let cast = Self::cast(src, &self.node(*lhs)?, &rhs)?;
                self.set_type(*id, cast.clone());
                cast
            }
            Node::Field { .. } => todo!(),
            Node::Call { id, target, args } => {
                let (tok, inner_name, fun) = match self.ast.node(*target) {
                    Node::Field { target, name, .. } => {
                        let Node::Ident {
                            name:
                                lex::Token {
                                    t: lex::Type::Ident(pkg_name),
                                    ..
                                },
                            ..
                        } = self.ast.node(*target)
                        else {
                            // TODO: add error handling for non ident call targets
                            unreachable!();
                        };

                        let lex::Token {
                            t: lex::Type::Ident(inner_name),
                            ..
                        } = name
                        else {
                            unreachable!();
                        };

                        let Some(pkg) = self.packages.get(pkg_name) else {
                            return Err(PgError::with_msg(
                                format!("Can't find package `{pkg_name}`"),
                                name,
                            ));
                        };

                        let Some(fun) = pkg.get(inner_name).cloned() else {
                            return Err(PgError::with_msg(
                                format!("Call to undefined function `{pkg_name}.{inner_name}`"),
                                name,
                            ));
                        };
                        let mut s = String::from(*pkg_name);
                        s.push('.');
                        s.push_str(inner_name);
                        (name, s, fun)
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
                            return Err(PgError::with_msg(
                                format!("Call to undefined function `{inner_name}`"),
                                name,
                            ));
                        };
                        (name, inner_name.to_string(), fun)
                    }
                    _ => unreachable!(),
                };

                if args.len() != fun.args.len() {
                    return Err(PgError::with_msg(
                        format!(
                            "`{}` requires {} arguments, got {}",
                            inner_name,
                            fun.args.len(),
                            args.len()
                        ),
                        tok,
                    ));
                }

                self.set_type(*id, fun.ret.clone());

                for (i, provided_node) in args.iter().enumerate() {
                    let provided_type = self.node(*provided_node)?;
                    let expected_type = &fun.args[i].1;

                    if expected_type != &provided_type {
                        return Err(PgError::with_msg(
                            format!(
                                "`{inner_name}` arg{i} expected {expected_type}, got {provided_type} instead",
                            ),
                            // TODO: extract this token from provided_node
                            tok,
                        ));
                    }
                }

                fun.ret
            }
            Node::Match { id, cases, default } => {
                // short circuit for empty matches
                if cases.is_empty() {
                    return Ok(Type::Void);
                }

                let case_count = cases.len();

                // all branches MUST resolve to the same type :)
                let mut branch_types: Vec<Option<(&Token, Type<'t>)>> =
                    vec![const { None }; case_count];

                for (i, ((condition_token, condition), body)) in cases.iter().enumerate() {
                    let condition_type: Type<'t> = self.node(*condition)?;

                    if condition_type != Type::Bool {
                        return Err(PgError::with_msg(
                            format!("Match conditions must be Bool, got {condition_type} instead"),
                            condition_token,
                        ));
                    }

                    let branch_return_type = self.block_type(body)?;
                    branch_types[i] = Some((condition_token, branch_return_type));
                }

                // we simply use the default branches type as the canonical type of the match, its
                // the easiest way to deal with this
                let first_type = self.block_type(&default.1)?;

                for cur in &branch_types {
                    let Some((tok, ty)) = cur else { unreachable!() };

                    if ty != &first_type {
                        return Err(PgError::with_msg(
                            format!(
                                "Match cases must resolve to the same type, but got {first_type} and {ty}"
                            ),
                            *tok,
                        ));
                    }
                }

                self.set_type(*id, first_type.clone());
                first_type
            }
            Node::Import { pkgs, src, .. } => {
                if pkgs.is_empty() {
                    return Err(PgError::with_msg(
                        "Import without any paths to import is considered invalid",
                        src,
                    ));
                }

                for pkg_tok in pkgs {
                    let lex::Type::S(pkg_name) = pkg_tok.t else {
                        unreachable!();
                    };

                    let Some(pkg) = self.resolve_pkg(pkg_name) else {
                        return Err(PgError::with_msg(
                            format!("Wasnt able to find a package named `{pkg_name}`"),
                            pkg_tok,
                        ));
                    };

                    purple_garden_shared::trace!("ty: resolved pkg `{}`", pkg.name);

                    let mut registered = HashMap::new();
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
                        purple_garden_shared::trace!(
                            "[ir::typecheck::Typechecker::node][{}.{}]: {}",
                            pkg.name,
                            f.name,
                            f_type
                        );
                        registered.insert(f.name, f_type);
                    }

                    self.packages.insert(pkg.name, registered);
                }

                Type::Void
            }
            Node::Array { .. } => todo!(),
            Node::Object { .. } => todo!(),
        })
    }
}
