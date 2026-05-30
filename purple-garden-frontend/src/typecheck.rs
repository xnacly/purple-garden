use std::{collections::HashMap, fmt::Display};

use crate::{
    ast::Node,
    err::PgError,
    lex::{self, Token},
};
use purple_garden_ir::ptype::Type;
use purple_garden_std as pstd;

#[must_use]
pub fn id_from_node(node: &Node) -> Option<usize> {
    Some(match node {
        Node::Atom { id, .. }
        | Node::Ident { id, .. }
        | Node::Bin { id, .. }
        | Node::Unary { id, .. }
        | Node::Array { id, .. }
        | Node::Object { id, .. }
        | Node::Let { id, .. }
        | Node::Match { id, .. }
        | Node::Call { id, .. }
        | Node::Cast { id, .. }
        | Node::Field { id, .. } => *id,
        Node::Fn { .. } | Node::Import { .. } => return None,
    })
}

#[derive(Debug, Clone)]
struct FunctionType {
    args: Vec<Type>,
    ret: Type,
}

impl Display for FunctionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(")?;
        for (i, t) in self.args.iter().enumerate() {
            if i + 1 == self.args.len() {
                write!(f, "{t}")?;
            } else {
                write!(f, "{t} ")?;
            }
        }
        write!(f, ") -> {}", self.ret)?;
        Ok(())
    }
}

#[derive(Default, Debug)]
pub struct Typechecker<'t> {
    /// Node id -> Type. Indexed by id; Node ids are dense from the parser.
    map: Vec<Option<Type>>,
    /// scope stack; innermost frame last; lookups walk from top to bottom
    env: Vec<HashMap<&'t str, Type>>,
    /// map a function name to its type(s)
    functions: HashMap<&'t str, FunctionType>,
    /// map a pkg name to a map of its methods and their types
    packages: HashMap<&'t str, HashMap<&'t str, FunctionType>>,
}

impl<'t> Typechecker<'t> {
    #[must_use]
    pub fn new() -> Self {
        let mut s = Self::default();
        s.env.push(HashMap::new());
        s
    }

    fn env_get(&self, k: &str) -> Option<&Type> {
        self.env.iter().rev().find_map(|frame| frame.get(k))
    }

    fn env_insert(&mut self, k: &'t str, v: Type) {
        self.env.last_mut().unwrap().insert(k, v);
    }

    #[must_use]
    pub fn finalise(self) -> Vec<Option<Type>> {
        self.map
    }

    fn set_type(&mut self, id: usize, t: Type) {
        if id >= self.map.len() {
            self.map.resize(id + 1, None);
        }
        self.map[id] = Some(t);
    }

    fn already_checked(&'t self, node: &Node) -> Option<&'t Type> {
        self.map.get(id_from_node(node)?).and_then(|o| o.as_ref())
    }

    fn fuse(op: &lex::Token, lhs: &Type, rhs: &Type) -> Result<Type, PgError> {
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

    fn cast(at: &lex::Token, i: &Type, o: &Type) -> Result<Type, PgError> {
        Ok(match (i, o) {
            (Type::Int, Type::Double) => Type::Double,
            (Type::Double | Type::Bool, Type::Int) => Type::Int,
            (Type::Int, Type::Bool) => Type::Bool,
            (_, _) => {
                return Err(PgError::with_msg(format!("Can not cast {i} to {o}"), at));
            }
        })
    }

    fn block_type(&mut self, nodes: &'t [Node]) -> Result<Type, PgError> {
        self.env.push(HashMap::new());
        let mut last_type = Type::Void;
        for node in nodes {
            last_type = self.node(node)?;
        }
        self.env.pop();
        Ok(last_type)
    }

    pub fn node(&mut self, node: &'t Node) -> Result<Type, PgError> {
        if let Some(t) = self.already_checked(node) {
            return Ok(t.clone());
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
                let lhs = self.node(lhs)?;
                let rhs = self.node(rhs)?;
                let res = Self::fuse(op, &lhs, &rhs)?;
                self.set_type(*id, res.clone());
                res
            }
            Node::Unary { id, op, rhs } => {
                let inner = self.node(rhs)?;
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
                let inner = self.node(rhs)?;
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

                    let t = crate::type_from_type_expr(arg_type);
                    self.env_insert(inner_name, t.clone());
                    typed_arguments.push(t);
                }

                let ret: Type = crate::type_from_type_expr(return_type);
                let f_type = FunctionType {
                    args: typed_arguments,
                    ret: ret.clone(),
                };
                self.functions.insert(inner_name, f_type.clone());

                let computed_ret = self.block_type(body)?;
                if ret != computed_ret {
                    return Err(PgError::with_msg(
                        format!("`{inner_name}` should return {ret}, but returns {computed_ret}"),
                        return_type,
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
                let rhs = crate::type_from_type_expr(rhs);
                let cast = Self::cast(src, &self.node(lhs)?, &rhs)?;
                self.set_type(*id, cast.clone());
                cast
            }
            Node::Field { .. } => todo!(),
            Node::Call { id, target, args } => {
                let (tok, inner_name, fun) = match target.as_ref() {
                    Node::Field { target, name, .. } => {
                        let Node::Ident {
                            name:
                                lex::Token {
                                    t: lex::Type::Ident(pkg_name),
                                    ..
                                },
                            ..
                        } = target.as_ref()
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
                    let provided_type = self.node(provided_node)?;
                    let expected_type = &fun.args[i];

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
                let mut branch_types: Vec<Option<(&Token, Type)>> =
                    vec![const { None }; case_count];

                for (i, ((condition_token, condition), body)) in cases.iter().enumerate() {
                    let condition_type: Type = self.node(condition)?;

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

                    let Some(pkg) = pstd::resolve_pkg(pkg_name) else {
                        return Err(PgError::with_msg(
                            format!("Wasnt able to find a package named `{pkg_name}`"),
                            pkg_tok,
                        ));
                    };

                    purple_garden_shared::trace!("ty: resolved pkg `{}`", pkg.name);

                    self.packages.insert(
                        pkg.name,
                        pkg.fns
                            .iter()
                            .map(|f| {
                                purple_garden_shared::trace!(
                                    "ty: registered `{}.{}`",
                                    pkg.name,
                                    f.name
                                );
                                (
                                    f.name,
                                    FunctionType {
                                        args: f.args.to_vec(),
                                        ret: f.ret.clone(),
                                    },
                                )
                            })
                            .collect(),
                    );
                }

                Type::Void
            }
            Node::Array { .. } => todo!(),
            Node::Object { .. } => todo!(),
        })
    }
}
