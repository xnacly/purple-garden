use std::{collections::HashMap, num};

use crate::{ast::Node, err::PgError, ir::*, lex::Type};

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

    fn new_functions(&mut self) -> Id {
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
}

impl<'lower> Lower<'lower> {
    pub fn new() -> Self {
        Self::default()
    }

    fn lower_node(&mut self, node: &'lower Node) -> Result<Option<Id>, PgError> {
        Ok(match node {
            Node::Atom { raw } => {
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
                self.current_func
                    .blocks
                    .get_mut(self.id_store.blocks)
                    .unwrap()
                    .instructions
                    .push(Instr::LoadConst {
                        dst: TypeId {
                            id,
                            ty: value.into(),
                        },
                        value,
                    });

                Some(id)
            }
            Node::Ident { name } => {
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
            Node::Bin { op, lhs, rhs } => {
                let Some(lhs) = self.lower_node(lhs)? else {
                    unreachable!()
                };
                let Some(rhs) = self.lower_node(rhs)? else {
                    unreachable!()
                };

                let id = self.id_store.new_value();
                let dst = TypeId {
                    id,
                    ty: ptype::Type::Int,
                };

                self.current_func
                    .blocks
                    .get_mut(self.id_store.blocks)
                    .unwrap()
                    .instructions
                    .push(match op.t {
                        Type::Plus => Instr::Add { dst, lhs, rhs },
                        Type::Minus => Instr::Sub { dst, lhs, rhs },
                        Type::Asteriks => Instr::Mul { dst, lhs, rhs },
                        Type::Slash => Instr::Div { dst, lhs, rhs },
                        Type::DoubleEqual => Instr::Eq { dst, lhs, rhs },
                        _ => unreachable!(),
                    });

                Some(id)
            }
            Node::Let { name, rhs } => {
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
            } => todo!(),
            _ => todo!("{:?}", node),
        })
    }

    /// Lower [ast] into a list of Func nodes, the entry point is always `__pg_entry`
    pub fn ir_from(&mut self, ast: &'lower [Node]) -> Result<Vec<Func<'lower>>, PgError> {
        // entry function
        let entry = Func {
            id: Id(0),
            entry: Id(0),
            ret: None,
            blocks: vec![Block {
                id: Id(0),
                instructions: vec![],
                params: vec![],
                term: Terminator::Return(None),
            }],
        };
        self.current_func = entry;

        for node in ast {
            let _ = &self.lower_node(node)?;
        }
        self.functions.push(self.current_func.clone());
        Ok(self.functions.clone())
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
