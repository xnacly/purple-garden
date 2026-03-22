use std::collections::HashMap;

use crate::{
    ast::Node,
    err::PgError,
    ir::ptype::Type,
    lex::{self, Token},
    std::{self as pstd, Pkg, STD},
};

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

#[derive(Default, Debug)]
pub struct Typechecker<'t> {
    /// maps each Node id to its computed type
    map: HashMap<usize, Type>,
    /// assign a variables name in the current scope to its type
    env: HashMap<&'t str, Type>,
    /// map a function name to its type(s)
    functions: HashMap<&'t str, FunctionType>,
    /// map a pkg name to a map of its methods and their types
    packages: HashMap<&'t str, HashMap<&'t str, FunctionType>>,
}

impl<'t> Typechecker<'t> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn finalise(self) -> HashMap<usize, Type> {
        self.map
    }

    fn already_checked(&'t self, node: &Node) -> Option<&'t Type> {
        self.map.get(&id_from_node(node)?)
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
                            "Type error",
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
                            "Type error",
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
            // comparing ints
            lex::Type::DoubleEqual | lex::Type::NotEqual => {
                match (lhs, rhs) {
                    (Type::Int, Type::Int) => {}
                    (_, _) if lhs == rhs => {
                        return Err(PgError::with_msg(
                            "Type error",
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
                            "Type error",
                            format!(
                                "Incompatible types {} and {} for {:?}, want Int",
                                lhs,
                                rhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                    }
                };
                Type::Bool
            }
            // comparing doubles
            lex::Type::LessThan | lex::Type::GreaterThan => {
                match (lhs, rhs) {
                    (Type::Double, Type::Double) => {}
                    (Type::Int, Type::Int) => {}
                    (_, _) if lhs == rhs => {
                        return Err(PgError::with_msg(
                            "Type error",
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
                            "Type error",
                            format!(
                                "Incompatible types {} and {} for {:?}",
                                lhs,
                                rhs,
                                op.t.as_str()
                            ),
                            op,
                        ));
                    }
                };
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
            (Type::Double, Type::Int) => Type::Int,
            (Type::Int, Type::Bool) => Type::Bool,
            (_, _) => {
                return Err(PgError::with_msg(
                    "Cast type error",
                    format!("Can not cast {} to {}", i, o),
                    at,
                ));
            }
        })
    }

    fn block_type(&mut self, nodes: &'t [Node]) -> Result<Type, PgError> {
        let mut last_type = Type::Void;
        for node in nodes {
            last_type = self.node(node)?;
        }
        Ok(last_type)
    }

    pub fn node(&mut self, node: &'t Node) -> Result<Type, PgError> {
        if let Some(t) = self.already_checked(node) {
            return Ok(t.clone());
        }

        Ok(match node {
            Node::Atom { id, raw } => {
                let t = Type::from_atom_token_type(&raw.t);
                self.map.insert(*id, t.clone());
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

                let t = self.env.get(inner_name).ok_or_else(|| {
                    PgError::with_msg(
                        "Undefined Binding",
                        format!("binding `{inner_name}` not found"),
                        name,
                    )
                })?;

                self.map.insert(*id, t.clone());
                t.clone()
            }
            Node::Bin { id, op, lhs, rhs } => {
                let lhs = self.node(lhs)?;
                let rhs = self.node(rhs)?;
                let res = Self::fuse(op, &lhs, &rhs)?;
                self.map.insert(*id, res.clone());
                res
            }
            Node::Unary { .. } => {
                todo!("{:?}", node);
            }
            Node::Let { id, name, rhs } => {
                let inner = self.node(rhs)?;
                self.map.insert(*id, inner.clone());
                let lex::Token {
                    t: lex::Type::Ident(inner_name),
                    ..
                } = name
                else {
                    unreachable!()
                };

                self.env.insert(inner_name, inner.clone());
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
                        "Function already defined",
                        format!("`{}` is already defined", inner_name),
                        name,
                    ));
                }

                let prev_env = std::mem::take(&mut self.env);
                let mut typed_arguments = Vec::with_capacity(args.len());
                for (arg_name, arg_type) in args {
                    let lex::Token {
                        t: lex::Type::Ident(inner_name),
                        ..
                    } = arg_name
                    else {
                        unreachable!()
                    };

                    let t: Type = arg_type.into();
                    self.env.insert(inner_name, t.clone());
                    typed_arguments.push(t);
                }

                let ret: Type = return_type.into();

                self.functions.insert(
                    inner_name,
                    FunctionType {
                        args: typed_arguments,
                        ret: ret.clone(),
                    },
                );

                let computed_ret = self.block_type(body)?;
                if ret != computed_ret {
                    return Err(PgError::with_msg(
                        "Function return type mismatch",
                        format!(
                            "`{}` annotated with return type {}, but returns {}",
                            inner_name, ret, computed_ret
                        ),
                        return_type,
                    ));
                }
                self.env = prev_env;
                ret
            }
            Node::Cast { id, lhs, rhs, src } => {
                let cast = Self::cast(src, &self.node(lhs)?, &rhs.into())?;
                self.map.insert(*id, cast.clone());
                cast
            }
            Node::Field { .. } => todo!(),
            Node::Call { id, target, args } => {
                let (tok, inner_name, fun) = match target.as_ref() {
                    Node::Field { id, target, name } => {
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
                                "Undefined package",
                                format!("Can't find package `{}`", pkg_name),
                                name,
                            ));
                        };

                        let Some(fun) = pkg.get(inner_name).cloned() else {
                            return Err(PgError::with_msg(
                                "Undefined function",
                                format!("Call to undefined function `{}.{}`", pkg_name, inner_name),
                                name,
                            ));
                        };
                        (name, inner_name, fun)
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
                                "Undefined function",
                                format!("Call to undefined function `{}`", inner_name),
                                name,
                            ));
                        };
                        (name, inner_name, fun)
                    }
                    _ => unreachable!(),
                };

                if args.len() != fun.args.len() {
                    return Err(PgError::with_msg(
                        "Function argument count mismatch",
                        format!(
                            "`{}` requires {} arguments, got {}",
                            inner_name,
                            fun.args.len(),
                            args.len()
                        ),
                        tok,
                    ));
                }

                self.map.insert(*id, fun.ret.clone());

                for (i, provided_node) in args.iter().enumerate() {
                    let provided_type = self.node(provided_node)?;
                    let expected_type = &fun.args[i];

                    if expected_type != &provided_type {
                        return Err(PgError::with_msg(
                            "Function argument type mismatch",
                            format!(
                                "`{}` arg{} expected type {}, got {} instead",
                                inner_name, i, expected_type, provided_type,
                            ),
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
                            "Non bool match condition",
                            format!(
                                "Match conditions must be Bool, got {} instead",
                                condition_type
                            ),
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
                            "Incompatible match case return type",
                            format!(
                                "Match cases must resolve to the same type, but got {} and {}",
                                first_type, ty
                            ),
                            *tok,
                        ));
                    };
                }

                self.map.insert(*id, first_type.clone());
                first_type
            }
            Node::Import { id, pkgs, src } => {
                if pkgs.is_empty() {
                    return Err(PgError::with_msg(
                        "Empty import statement",
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
                            "Unresolvable pkg import",
                            format!("Wasnt able to find a package named `{pkg_name}`"),
                            pkg_tok,
                        ));
                    };

                    crate::trace!("ty: resolved pkg `{}`", pkg.name);

                    self.packages.insert(
                        pkg.name,
                        pkg.fns
                            .iter()
                            .map(|f| {
                                crate::trace!("ty: registered `{}.{}`", pkg.name, f.name);
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
            Node::Array { id, members } => todo!(),
            Node::Object { id, pairs } => todo!(),
        })
    }
}
