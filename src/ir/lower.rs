use std::{collections::HashMap, num};

use crate::{
    ast::{Node, TypeExpr},
    err::PgError,
    ir::*,
    lex::{Token, Type},
};

#[derive(Default)]
struct IdStore {
    values: usize,
    blocks: usize,
    functions: usize,
}

impl IdStore {
    fn new_value(&mut self) -> Id {
        let val = self.values;
        self.values += 1;
        Id(val as u32)
    }

    fn new_block(&mut self) -> Id {
        let blk = self.blocks;
        self.blocks += 1;
        Id(blk as u32)
    }

    fn new_function(&mut self) -> Id {
        let fun = self.functions;
        self.functions += 1;
        Id(fun as u32)
    }
}

#[derive(Default)]
pub struct Lower<'lower> {
    functions: Vec<Func<'lower>>,
    current_func: Func<'lower>,
    id_store: IdStore,
    /// maps ast variable names to ssa values
    env: HashMap<&'lower str, Id>,
    func_name_to_id: HashMap<&'lower str, Id>,
    types: HashMap<usize, ptype::Type>,
}

impl<'lower> Lower<'lower> {
    pub fn new() -> Self {
        Self::default()
    }

    fn emit(&mut self, i: Instr<'lower>) {
        self.current_func
            .blocks
            .last_mut()
            .unwrap()
            .instructions
            .push(i);
    }

    fn lower_node(&mut self, node: &'lower Node) -> Result<Option<Id>, PgError> {
        Ok(match node {
            Node::Atom { raw, id } => {
                let value = match raw.t {
                    Type::S(str) => Const::Str(str),
                    Type::D(doub) => Const::Double(
                        doub.parse::<f64>()
                            .map_err(|e: num::ParseFloatError| {
                                PgError::with_msg(e.to_string(), raw)
                            })?
                            .to_bits(),
                    ),
                    Type::I(int) => {
                        Const::Int(int.parse().map_err(|e: num::ParseIntError| {
                            PgError::with_msg(e.to_string(), raw)
                        })?)
                    }
                    Type::True => Const::True,
                    Type::False => Const::False,
                    _ => unreachable!(),
                };

                let id = self.id_store.new_value();
                self.emit(Instr::LoadConst {
                    dst: TypeId {
                        id,
                        ty: value.into(),
                    },
                    value,
                });

                Some(id)
            }
            Node::Ident { name, id } => {
                let Type::Ident(i) = name.t else {
                    unreachable!()
                };
                if let Some(id) = self.env.get(i) {
                    Some(*id)
                } else {
                    return Err(PgError::with_msg(
                        format!("Undefined variable `{}`", i),
                        name,
                    ));
                }
            }
            Node::Bin { op, lhs, rhs, id } => {
                let Some(lhs) = self.lower_node(lhs)? else {
                    unreachable!()
                };
                let Some(rhs) = self.lower_node(rhs)? else {
                    unreachable!()
                };

                let dst_id = self.id_store.new_value();
                let dst = TypeId {
                    id: dst_id,
                    ty: self.types.get(id).unwrap().clone(),
                };

                self.emit(match op.t {
                    Type::Plus => Instr::Add { dst, lhs, rhs },
                    Type::Minus => Instr::Sub { dst, lhs, rhs },
                    Type::Asteriks => Instr::Mul { dst, lhs, rhs },
                    Type::Slash => Instr::Div { dst, lhs, rhs },
                    Type::DoubleEqual => Instr::Eq { dst, lhs, rhs },
                    _ => unreachable!(),
                });

                Some(dst_id)
            }
            Node::Let { name, rhs, id } => {
                let Type::Ident(i) = name.t else {
                    unreachable!()
                };
                if let Some(id) = self.lower_node(rhs)? {
                    self.env.insert(i, id);
                } else {
                    return Err(PgError::with_msg(
                        "RHS of let has to return a value, but it didnt",
                        name,
                    ));
                }
                None
            }
            Node::Fn {
                name,
                args,
                return_type,
                body,
            } => {
                let old_func = std::mem::take(&mut self.current_func);
                let old_env = std::mem::take(&mut self.env);
                let id = self.id_store.new_function();
                let Type::Ident(ident_name) = name.t else {
                    unreachable!()
                };
                self.func_name_to_id.insert(ident_name, id);

                let func = Func {
                    id,
                    blocks: vec![
                        // entry block
                        Block {
                            id: self.id_store.new_block(),
                            instructions: vec![],
                            params: args
                                .iter()
                                .map(|(token, token_type)| {
                                    let id = self.id_store.new_value();
                                    let Type::Ident(ident) = token.t else {
                                        unreachable!();
                                    };
                                    self.env.insert(ident, id);
                                    TypeId {
                                        id,
                                        ty: token_type.into(),
                                    }
                                })
                                .collect(),
                            term: Terminator::Return(None),
                        },
                    ],
                    ret: if let TypeExpr::Atom(Token { t: Type::Void, .. }) = return_type {
                        None
                    } else {
                        if let TypeExpr::Atom(Token { t, .. }) = return_type {
                            Some((*t).into())
                        } else {
                            None
                        }
                    },
                };

                self.current_func = func;
                let mut last_id = None;
                for node in body {
                    last_id = self.lower_node(node)?;
                }
                if last_id.is_some() {
                    self.current_func.blocks.last_mut().unwrap().term = Terminator::Return(last_id);
                }

                self.functions.push(std::mem::take(&mut self.current_func));
                self.env = old_env;
                self.current_func = old_func;
                None
            }
            Node::Call { name, args, id } => {
                let Type::Ident(ident_name) = name.t else {
                    unreachable!()
                };

                let Some(target_id) = self.func_name_to_id.get(ident_name).cloned() else {
                    return Err(PgError::with_msg(
                        format!("Undefined function `{ident_name}`"),
                        name,
                    ));
                };

                let mut a = vec![];
                for arg in args {
                    let Some(id) = self.lower_node(arg)? else {
                        unreachable!();
                    };
                    a.push(id);
                }

                let dst = self.id_store.new_value();
                self.emit(Instr::Call {
                    dst: Some(dst),
                    func: target_id,
                    args: a,
                });

                Some(dst)
            }
            Node::Cast { id, lhs, rhs, .. } => {
                let Some(from) = self.lower_node(lhs)? else {
                    unreachable!()
                };

                let dst = self.id_store.new_value();
                let value = TypeId {
                    id: dst,
                    ty: rhs.into(),
                };

                self.emit(Instr::Cast { value, from });
                Some(dst)
            }
            _ => todo!("{:?}", node),
        })
    }

    /// Lower [ast] into a list of Func nodes, the entry point is always `__pg_entry`
    pub fn ir_from(mut self, ast: &'lower [Node]) -> Result<Vec<Func<'lower>>, PgError> {
        let mut typechecker = typecheck::Typechecker::new();
        for node in ast {
            typechecker.node(node)?;
        }
        trace!("{:#?}", typechecker);
        self.types = typechecker.finalise();

        // entry function
        self.current_func = Func {
            id: self.id_store.new_function(),
            ret: None,
            blocks: vec![Block {
                id: self.id_store.new_block(),
                instructions: vec![],
                params: vec![],
                term: Terminator::Return(None),
            }],
        };

        for node in ast {
            let _ = &self.lower_node(node)?;
        }
        self.functions.push(self.current_func);

        Ok(self.functions)
    }
}

#[cfg(test)]
mod lower {
    #[test]
    fn atom() {}
    #[test]
    fn ident() {}
    #[test]
    fn bin() {}
    #[test]
    fn r#let() {}
    #[test]
    fn r#fn() {}
    #[test]
    fn r#call() {}
    #[test]
    fn r#match() {}
    #[test]
    fn path() {}
}
