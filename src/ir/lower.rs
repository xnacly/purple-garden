use std::{collections::HashMap, num};

use crate::{
    ast::{Node, TypeExpr},
    err::PgError,
    ir::{self, typecheck::id_from_node, *},
    lex::{self, Token, Type},
    std as pstd,
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
struct LowerCtx<'lower> {
    /// current function
    func: Func<'lower>,
    /// current block
    block: Id,
    id_store: IdStore,
    /// maps ast variable names to ssa values
    env: HashMap<&'lower str, Id>,
}

#[derive(Default)]
pub struct Lower<'lower> {
    ctx: LowerCtx<'lower>,
    functions: Vec<Func<'lower>>,
    func_name_to_id: HashMap<&'lower str, (ir::Id, Option<ptype::Type>)>,
    types: HashMap<usize, ptype::Type>,
    packages: HashMap<&'lower str, (&'lower pstd::Pkg, HashMap<&'lower str, &'lower pstd::Fn>)>,
}

impl<'lower> Lower<'lower> {
    pub fn new() -> Self {
        Self::default()
    }

    fn emit(&mut self, i: Instr<'lower>) {
        self.ctx.func.blocks[self.ctx.block.0 as usize]
            .instructions
            .push(i);
    }

    fn cur(&self) -> &Block<'lower> {
        let ir::Id(idx) = self.ctx.block;
        self.ctx.func.blocks.get(idx as usize).unwrap()
    }

    fn new_block(&mut self) -> Id {
        let id = Id(self.ctx.func.blocks.len() as u32);
        self.ctx.func.blocks.push(Block {
            id,
            tombstone: false,
            instructions: vec![],
            params: vec![],
            term: None,
        });
        id
    }

    fn block_mut(&mut self, id: Id) -> &mut Block<'lower> {
        &mut self.ctx.func.blocks[id.0 as usize]
    }

    fn switch_to_block(&mut self, id: Id) {
        self.ctx.block = id
    }

    fn lower_node(&mut self, node: &Node<'lower>) -> Result<Option<Id>, PgError> {
        Ok(match node {
            Node::Atom { raw, .. } => {
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

                let id = self.ctx.id_store.new_value();
                self.emit(Instr::LoadConst {
                    dst: TypeId {
                        id,
                        ty: value.into(),
                    },
                    value,
                });

                Some(id)
            }
            Node::Ident { name, .. } => {
                let Type::Ident(i) = name.t else {
                    unreachable!()
                };
                if let Some(id) = self.ctx.env.get(i) {
                    Some(*id)
                } else {
                    return Err(PgError::with_msg(
                        format!("Undefined variable `{}`", i),
                        name,
                    ));
                }
            }
            Node::Bin { op, lhs, rhs, id } => {
                let src_type = self
                    .types
                    .get(&id_from_node(lhs).unwrap())
                    .cloned()
                    .unwrap();

                let Some(lhs) = self.lower_node(lhs)? else {
                    unreachable!()
                };
                let Some(rhs) = self.lower_node(rhs)? else {
                    unreachable!()
                };

                let dst_id = self.ctx.id_store.new_value();
                let dst = TypeId {
                    id: dst_id,
                    ty: self.types.get(id).unwrap().clone(),
                };

                use BinOp::*;
                let op = match src_type {
                    ptype::Type::Bool => match op.t {
                        Type::DoubleEqual => BEq,
                        _ => unreachable!(),
                    },
                    ptype::Type::Int => match op.t {
                        Type::Plus => IAdd,
                        Type::Minus => ISub,
                        Type::Asteriks => IMul,
                        Type::Slash => IDiv,
                        Type::DoubleEqual => IEq,
                        Type::LessThan => ILt,
                        Type::GreaterThan => IGt,
                        _ => unreachable!(),
                    },
                    ptype::Type::Double => match op.t {
                        Type::Plus => DAdd,
                        Type::Minus => DSub,
                        Type::Asteriks => DMul,
                        Type::Slash => DDiv,
                        Type::LessThan => DLt,
                        Type::GreaterThan => DGt,
                        _ => unreachable!(),
                    },
                    _ => todo!("{:#?}", src_type),
                };

                self.emit(Instr::Bin { op, dst, lhs, rhs });

                Some(dst_id)
            }
            Node::Unary { op, rhs, .. } => {
                let inner_ty = self
                    .types
                    .get(&id_from_node(rhs).unwrap())
                    .cloned()
                    .unwrap();
                let Some(rhs_id) = self.lower_node(rhs)? else {
                    unreachable!()
                };

                match op.t {
                    Type::Plus => Some(rhs_id),
                    Type::Minus => {
                        let (zero_const, bin_op) = match inner_ty {
                            ptype::Type::Int => (Const::Int(0), BinOp::ISub),
                            ptype::Type::Double => (Const::Double(0u64), BinOp::DSub),
                            _ => unreachable!(),
                        };
                        let zero_id = self.ctx.id_store.new_value();
                        self.emit(Instr::LoadConst {
                            dst: TypeId {
                                id: zero_id,
                                ty: inner_ty.clone(),
                            },
                            value: zero_const,
                        });

                        let dst_id = self.ctx.id_store.new_value();
                        self.emit(Instr::Bin {
                            op: bin_op,
                            dst: TypeId {
                                id: dst_id,
                                ty: inner_ty,
                            },
                            lhs: zero_id,
                            rhs: rhs_id,
                        });
                        Some(dst_id)
                    }
                    _ => unreachable!(),
                }
            }
            Node::Let { name, rhs, .. } => {
                let Type::Ident(i) = name.t else {
                    unreachable!()
                };
                if let Some(id) = self.lower_node(rhs)? {
                    self.ctx.env.insert(i, id);
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
                let old_ctx = std::mem::take(&mut self.ctx);

                let id = Id(self.functions.len() as u32 + 1);
                let Type::Ident(ident_name) = name.t else {
                    unreachable!()
                };

                let ret = if let TypeExpr::Atom(Token { t: Type::Void, .. }) = return_type {
                    None
                } else {
                    if let TypeExpr::Atom(Token { t, .. }) = return_type {
                        Some((*t).into())
                    } else {
                        None
                    }
                };

                self.func_name_to_id.insert(ident_name, (id, ret.clone()));
                let func = Func {
                    name: ident_name,
                    id,
                    params: args
                        .iter()
                        .map(|(token, _)| {
                            let id = self.ctx.id_store.new_value();
                            let Type::Ident(ident) = token.t else {
                                unreachable!();
                            };
                            self.ctx.env.insert(ident, id);
                            id
                        })
                        .collect(),
                    blocks: vec![],
                    ret,
                };

                // TODO:deal with b0

                self.ctx.func = func;
                let entry = self.new_block();
                self.block_mut(entry).params = self.ctx.func.params.clone();

                let mut last = None;
                for node in body {
                    self.switch_to_block(entry);
                    last = self.lower_node(node)?;
                }

                if self.ctx.func.ret.is_some() {
                    self.block_mut(self.ctx.block).term = Some(Terminator::Return(last));
                } else {
                    self.block_mut(self.ctx.block).term = Some(Terminator::Return(None));
                }

                self.functions.push(std::mem::take(&mut self.ctx.func));
                self.ctx = old_ctx;
                None
            }
            Node::Call { target, args, .. } => {
                let mut a = vec![];
                for arg in args {
                    let Some(id) = self.lower_node(arg)? else {
                        unreachable!();
                    };
                    a.push(id);
                }

                let dst_id = self.ctx.id_store.new_value();
                let mut dst = TypeId {
                    // this is a placeholder
                    ty: ptype::Type::Void,
                    id: dst_id,
                };

                match target.as_ref() {
                    // 'syscall' / stdlib call
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
                            unreachable!();
                        };

                        let lex::Token {
                            t: lex::Type::Ident(inner_name),
                            ..
                        } = name
                        else {
                            unreachable!();
                        };

                        // both unwrappable because the typechecker makes sure everything is fine
                        let fun = self
                            .packages
                            .get(pkg_name)
                            .unwrap()
                            .1
                            .get(inner_name)
                            .unwrap();

                        dst.ty = fun.ret.clone();
                        self.emit(Instr::Sys {
                            dst,
                            path: pkg_name,
                            func: fun,
                            args: a,
                        });
                    }
                    // user defined function
                    Node::Ident { name, .. } => {
                        let crate::lex::Token {
                            t: crate::lex::Type::Ident(inner_name),
                            ..
                        } = name
                        else {
                            unreachable!();
                        };

                        let Some((target_id, ret)) = self.func_name_to_id.get(inner_name).cloned()
                        else {
                            return Err(PgError::with_msg(
                                format!("Undefined function `{inner_name}`"),
                                name,
                            ));
                        };

                        dst.ty = ret.unwrap_or(ptype::Type::Void);
                        self.emit(Instr::Call {
                            dst,
                            func: target_id,
                            args: a,
                        });
                    }
                    _ => unreachable!(),
                };

                Some(dst_id)
            }
            Node::Import { src, pkgs, .. } => {
                for pkg_tok in pkgs {
                    let Token {
                        t: Type::S(as_str), ..
                    } = pkg_tok
                    else {
                        unreachable!();
                    };

                    // the type checker already checks all packages are valid
                    let Some(pkg) = pstd::resolve_pkg(as_str) else {
                        unreachable!()
                    };

                    self.packages
                        .insert(as_str, (pkg, pkg.fns.iter().map(|f| (f.name, f)).collect()));
                }
                None
            }
            Node::Cast { lhs, rhs, .. } => {
                let src_ty = id_from_node(lhs)
                    .and_then(|aid| self.types.get(&aid).cloned())
                    .expect("typechecker should have typed the cast's lhs");

                let Some(from_id) = self.lower_node(lhs)? else {
                    unreachable!()
                };

                let dst = self.ctx.id_store.new_value();
                let value = TypeId {
                    id: dst,
                    ty: rhs.into(),
                };

                self.emit(Instr::Cast {
                    dst: value,
                    from: TypeId {
                        id: from_id,
                        ty: src_ty,
                    },
                });
                Some(dst)
            }
            Node::Match { cases, default, .. } => {
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

                let default_block = self.block_mut(default_block);
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
    pub fn ir_from(mut self, ast: &[Node<'lower>]) -> Result<Vec<Func<'lower>>, PgError> {
        let mut typechecker = typecheck::Typechecker::new();
        for node in ast {
            let _t = typechecker.node(node)?;
        }
        crate::trace!("[ir::lower::Lower::ir_from] Finished type checking");
        self.types = typechecker.finalise();

        self.ctx.func = Func {
            id: Id(0),
            name: "entry",
            ret: None,
            blocks: vec![],
            params: vec![],
        };
        let entry = self.new_block();
        self.switch_to_block(entry);

        for node in ast {
            let _ = &self.lower_node(node)?;
            // reset to the main entry point block to keep emitting nodes into the correct conext
            self.switch_to_block(entry);
        }

        self.functions.push(self.ctx.func);

        Ok(self.functions)
    }
}
