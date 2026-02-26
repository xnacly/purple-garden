use std::{collections::HashMap, num};

use crate::{
    ast::{Node, TypeExpr},
    err::PgError,
    ir::{self, *},
    lex::{Token, Type},
    vm::op::Op,
};

#[derive(Default)]
struct IdStore {
    values: usize,
}

impl IdStore {
    fn new_value(&mut self) -> Id {
        let val = self.values;
        self.values += 1;
        Id(val as u32)
    }
}

#[derive(Default)]
pub struct Lower<'lower> {
    functions: Vec<Func<'lower>>,
    /// current function
    func: Func<'lower>,
    /// current block
    block: Id,
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
        self.func
            .blocks
            .get_mut(self.block.0 as usize)
            .unwrap()
            .instructions
            .push(i);
    }

    fn cur(&self) -> &Block<'lower> {
        let ir::Id(idx) = self.block;
        self.func.blocks.get(idx as usize).unwrap()
    }

    fn new_block(&mut self) -> Id {
        let id = Id(self.func.blocks.len() as u32);
        self.func.blocks.push(Block {
            id,
            tombstone: false,
            instructions: vec![],
            params: vec![],
            term: None,
        });
        id
    }

    fn block_mut(&mut self, id: Id) -> &mut Block<'lower> {
        self.func.blocks.iter_mut().find(|b| b.id == id).unwrap()
    }

    fn switch_to_block(&mut self, id: Id) {
        self.block = id
    }

    fn lower_node(&mut self, node: &'lower Node) -> Result<Option<Id>, PgError> {
        Ok(match node {
            Node::Atom { raw, id } => {
                let value = match raw.t {
                    Type::S(str) => Const::Str(str),
                    Type::D(doub) => Const::Double(
                        doub.parse::<f64>()
                            .map_err(|e: num::ParseFloatError| {
                                PgError::with_msg("Number parsing failure", e.to_string(), raw)
                            })?
                            .to_bits(),
                    ),
                    Type::I(int) => Const::Int(int.parse().map_err(|e: num::ParseIntError| {
                        PgError::with_msg("Number parsing failure", e.to_string(), raw)
                    })?),
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
                        "Undefined binding",
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
                    Type::LessThan => Instr::Lt { dst, lhs, rhs },
                    Type::GreaterThan => Instr::Gt { dst, lhs, rhs },
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
                        "Empty binding value",
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
                let old_func = std::mem::take(&mut self.func);
                let old_env = std::mem::take(&mut self.env);
                let old_store = std::mem::take(&mut self.id_store);
                let old_block = std::mem::take(&mut self.block);

                let id = Id(self.functions.len() as u32 + 1);
                let Type::Ident(ident_name) = name.t else {
                    unreachable!()
                };
                self.func_name_to_id.insert(ident_name, id);

                let func = Func {
                    name: ident_name,
                    id,
                    blocks: vec![],
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

                self.func = func;
                let entry = self.new_block();
                self.block_mut(entry).params = args
                    .iter()
                    .map(|(token, token_type)| {
                        let id = self.id_store.new_value();
                        let Type::Ident(ident) = token.t else {
                            unreachable!();
                        };
                        self.env.insert(ident, id);
                        id
                    })
                    .collect();

                let mut last = None;
                for node in body {
                    self.switch_to_block(entry);
                    last = self.lower_node(node)?;
                }

                if self.func.ret.is_some() {
                    self.block_mut(self.block).term = Some(Terminator::Return(last));
                } else {
                    self.block_mut(self.block).term = Some(Terminator::Return(None));
                }

                self.functions.push(std::mem::take(&mut self.func));
                self.env = old_env;
                self.func = old_func;
                self.id_store = old_store;
                self.block = old_block;
                None
            }
            Node::Call { name, args, id } => {
                let Type::Ident(ident_name) = name.t else {
                    unreachable!()
                };

                let Some(target_id) = self.func_name_to_id.get(ident_name).cloned() else {
                    return Err(PgError::with_msg(
                        "Undefined function",
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
                    dst,
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
            Node::Match { cases, default, id } => {
                let mut check_blocks = Vec::with_capacity(cases.len());
                let mut body_blocks = Vec::with_capacity(cases.len());

                // pre"allocating" ebbs
                for _ in cases {
                    check_blocks.push(self.new_block());
                    body_blocks.push(self.new_block());
                }

                let params = self.cur().params.clone();

                // INFO:
                // this is only for correctness to jump into the match statements first check, we
                // will just leave the block empty and add no terminator, meaning it will be
                // skipped fully
                //
                // self.block_mut(self.block).term = Some(Terminator::Jump {
                //     id: *check_blocks.first().unwrap(),
                //     params: params.clone(),
                // });

                // the default block
                let default_block = self.new_block();

                // the single join block, merging all value results into a single branch
                let join = self.new_block();

                for (i, ((_, condition), body)) in cases.iter().enumerate() {
                    self.switch_to_block(check_blocks[i]);
                    let Some(cond) = self.lower_node(condition)? else {
                        unreachable!(
                            "Compiler bug, match cases MUST have a condition returning a value"
                        );
                    };

                    let no_target = if i + 1 < cases.len() {
                        check_blocks[i + 1]
                    } else {
                        default_block
                    };

                    let check_block_mut = self.block_mut(check_blocks[i]);
                    check_block_mut.term = Some(Terminator::Branch {
                        cond,
                        yes: (body_blocks[i], params.clone()),
                        no: (no_target, params.clone()),
                    });
                    check_block_mut.params = params.clone();

                    self.switch_to_block(body_blocks[i]);
                    self.block_mut(body_blocks[i]).params = params.clone();
                    let mut last = None;
                    for node in body {
                        last = self.lower_node(node)?;
                    }
                    let value = last.expect("match body must produce value");

                    self.block_mut(body_blocks[i]).term = Some(Terminator::Jump {
                        id: join,
                        params: vec![value],
                    });
                }

                // the typechecker checked we have a default case, so this is safe
                let (_, body) = default;
                self.switch_to_block(default_block);
                let mut last = None;
                for node in body.iter() {
                    last = self.lower_node(node)?;
                }
                let mut default_block = self.block_mut(default_block);
                default_block.params = params;
                let last = last.expect("match default must produce value");
                default_block.term = Some(Terminator::Jump {
                    id: join,
                    params: vec![last],
                });

                self.switch_to_block(join);
                self.block_mut(join).params = vec![last];
                Some(last)
            }
            _ => todo!("{:?}", node),
        })
    }

    /// Lower [ast] into a list of Func nodes, the entry point is always `entry`
    pub fn ir_from(mut self, ast: &'lower [Node]) -> Result<Vec<Func<'lower>>, PgError> {
        let mut typechecker = typecheck::Typechecker::new();
        for node in ast {
            let t = typechecker.node(node)?;
            crate::trace!("{} resolved to {:?}", &node, t);
        }
        crate::trace!("Finished type checking");
        self.types = typechecker.finalise();

        self.func = Func {
            id: Id(0),
            name: "entry",
            ret: None,
            blocks: vec![],
        };
        let entry = self.new_block();
        self.switch_to_block(entry);

        for node in ast {
            let _ = &self.lower_node(node)?;
            // reset to the main entry point block to keep emitting nodes into the correct conext
            self.switch_to_block(entry);
        }

        self.functions.push(self.func);
        Ok(self.functions)
    }
}
