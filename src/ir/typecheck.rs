use std::collections::HashMap;

use crate::{ast::Node, err::PgError, ir::ptype::Type, lex};

fn id_from_node(node: &Node) -> Option<usize> {
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
        | Node::Cast { id, .. } => *id,
        Node::Fn { .. } => return None,
    })
}

#[derive(Debug, Clone)]
struct FunctionType {
    args: Vec<Type>,
    ret: Type,
}

#[derive(Default, Debug)]
pub struct Typechecker<'t> {
    map: HashMap<usize, Type>,
    env: HashMap<&'t str, Type>,
    functions: HashMap<&'t str, FunctionType>,
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
                    (_, _) => {
                        return Err(PgError::with_msg(
                            "Type error",
                            format!("Incompatible types {} and {} for {:?}", lhs, rhs, op.t),
                            op,
                        ));
                    }
                }
            }
            // boolish operations
            lex::Type::LessThan
            | lex::Type::GreaterThan
            | lex::Type::DoubleEqual
            | lex::Type::NotEqual => {
                if lhs != rhs {
                    return Err(PgError::with_msg(
                        "Type error",
                        format!("Incompatible types {} and {} for {:?}", lhs, rhs, op.t),
                        op,
                    ));
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
            (Type::Double, Type::Int) => Type::Int,
            (_, _) => {
                return Err(PgError::with_msg(
                    "Cast type error",
                    format!("Can not cast {} to {}", i, o),
                    at,
                ));
            }
        })
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
            Node::Unary { id, op, rhs } => {
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
                let lex::Token {
                    t: lex::Type::Ident(inner_name),
                    ..
                } = name
                else {
                    unreachable!()
                };

                for node in body {
                    self.node(node)?;
                }

                // TODO: verify this
                let ret: Type = return_type.into();

                self.functions.insert(
                    inner_name,
                    FunctionType {
                        args: typed_arguments,
                        ret: ret.clone(),
                    },
                );
                self.env = prev_env;
                ret
            }
            Node::Cast { id, lhs, rhs, src } => Self::cast(src, &self.node(lhs)?, &rhs.into())?,
            Node::Call { id, name, args } => {
                let lex::Token {
                    t: lex::Type::Ident(inner_name),
                    ..
                } = name
                else {
                    unreachable!()
                };

                let Some(fun) = self.functions.get(inner_name).cloned() else {
                    return Err(PgError::with_msg(
                        "Undefined function",
                        format!("Call to undefined function `{}`", inner_name),
                        name,
                    ));
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
                        name,
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
                            name,
                        ));
                    }
                }

                fun.ret
            }
            _ => todo!("{:?}", node),
        })
    }
}
