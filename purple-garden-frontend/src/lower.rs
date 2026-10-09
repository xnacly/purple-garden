// Diagnostics are returned by value throughout lowering to preserve its
// allocation-free error path; boxing them would make the internal API noisier.
#![allow(clippy::result_large_err)]

use std::{
    alloc::{Allocator, Global, Layout},
    collections::HashMap,
    hash::RandomState,
    num,
};

type Map<K, V, S> = HashMap<K, V, RandomState, S>;

use crate::typemap::TypeMap;
use crate::{
    ast::{Ast, Node, NodeId, TypeExpr},
    diagnostic::Diagnostic,
    lex::{self, Token, Type},
};
use purple_garden_ir::{
    BinOp, Block, Const, EMPTY_PARAMS, Func, Id, Instr, Terminator, TypeId, ptype,
};
use purple_garden_runtime::Pkg;
use purple_garden_std as pstd;

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

fn align_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (value + align - 1) & !(align - 1)
}

struct LowerCtx<'lower, S: Allocator> {
    /// current function
    func: Func<'lower>,
    /// current block
    block: Id,
    id_store: IdStore,
    /// maps ast variable names to ssa values
    env: Map<&'lower str, Id, S>,
}

impl<S: Allocator> LowerCtx<'_, S> {
    fn new_in(scratch: S) -> Self {
        Self {
            func: Func::default(),
            block: Id::default(),
            id_store: IdStore::default(),
            env: Map::new_in(scratch),
        }
    }
}

type Overloads<'lower, S> = Map<&'lower str, Vec<&'lower pstd::Fn<'static>, S>, S>;

/// Builds the IR, its working state lives in the scratch allocator `S`.
pub struct Lower<'lower, A: Allocator = Global, S: Allocator + Clone = Global> {
    scratch: S,
    ctx: LowerCtx<'lower, S>,
    functions: Vec<Func<'lower>>,
    func_name_to_id: Map<&'lower str, (Id, Option<ptype::Type<'lower>>), S>,
    types: TypeMap<'lower, A>,
    packages: Map<&'lower str, (&'lower Pkg, Overloads<'lower, S>), S>,
    pkg_cache: Map<&'lower str, Option<&'lower Pkg>, S>,
    libs: Vec<&'lower Pkg>,
    stdlib: &'lower [purple_garden_runtime::Pkg],
}

impl Default for Lower<'_> {
    fn default() -> Self {
        Self::new_in(Global)
    }
}

impl Lower<'_> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<S: Allocator + Clone> Lower<'_, Global, S> {
    #[must_use]
    pub fn new_in(scratch: S) -> Self {
        Self {
            ctx: LowerCtx::new_in(scratch.clone()),
            functions: Vec::new(),
            func_name_to_id: Map::new_in(scratch.clone()),
            types: TypeMap::default(),
            packages: Map::new_in(scratch.clone()),
            pkg_cache: Map::new_in(scratch.clone()),
            libs: Vec::new(),
            stdlib: pstd::STD,
            scratch,
        }
    }
}

impl<'lower, A: Allocator, S: Allocator + Clone> Lower<'lower, A, S> {
    #[must_use]
    pub fn with_libs(mut self, libs: Vec<&'lower Pkg>) -> Self {
        self.libs = libs;
        self
    }

    #[must_use]
    pub fn with_stdlib_enabled(mut self, stdlib: bool) -> Self {
        self.stdlib = if stdlib { pstd::STD } else { &[] };
        self
    }

    #[must_use]
    pub fn with_stdlib(mut self, stdlib: &'lower [Pkg]) -> Self {
        self.stdlib = stdlib;
        self
    }

    fn resolve_pkg(&mut self, query: &'lower str) -> Option<&'lower Pkg> {
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

    fn emit(&mut self, i: Instr<'lower>) {
        self.ctx.func.blocks[self.ctx.block.0 as usize]
            .instructions
            .push(i);
    }

    fn cur(&self) -> &Block<'lower> {
        let Id(idx) = self.ctx.block;
        self.ctx.func.blocks.get(idx as usize).unwrap()
    }

    fn new_block(&mut self) -> Id {
        let id = Id(self.ctx.func.blocks.len() as u32);
        self.ctx.func.blocks.push(Block {
            id,
            tombstone: false,
            instructions: vec![],
            params: EMPTY_PARAMS,
            term: None,
        });
        id
    }

    fn block_mut(&mut self, id: Id) -> &mut Block<'lower> {
        &mut self.ctx.func.blocks[id.0 as usize]
    }

    fn switch_to_block(&mut self, id: Id) {
        self.ctx.block = id;
    }

    fn lower_node_into(
        &mut self,
        ast: &'lower Ast<'lower, 'lower>,
        node_id: NodeId,
        ty: &ptype::Type<'lower>,
        base: Id,
        offset: u32,
        span: u32,
    ) -> Result<(), Diagnostic> {
        if let Node::Record { fields, .. } = ast.node(node_id) {
            let ptype::Type::Record(_) = ty else {
                unreachable!("record literal was typechecked as non-record")
            };

            for (tok, value) in *fields {
                let lex::Type::Ident(name) = tok.t else {
                    unreachable!();
                };
                let field_offset =
                    offset + ty.field_offset(name).expect("record field was typechecked") as u32;
                let field_ty = self.types.get(ast.value_id(*value)).cloned().unwrap();
                self.lower_node_into(ast, *value, &field_ty, base, field_offset, tok.start as u32)?;
            }

            return Ok(());
        }

        let Some(src) = self.lower_node(ast, node_id)? else {
            unreachable!("node lowered into memory must produce a value");
        };

        self.emit(Instr::Store {
            src,
            base,
            offset,
            span,
        });

        Ok(())
    }

    fn lower_node(
        &mut self,
        ast: &'lower Ast<'lower, 'lower>,
        node_id: NodeId,
    ) -> Result<Option<Id>, Diagnostic> {
        let node = ast.node(node_id);
        Ok(match node {
            Node::Atom { raw, .. } => {
                let value = match raw.t {
                    Type::S(str) => Const::from(str),
                    Type::D(doub) => Const::Double(
                        doub.parse::<f64>()
                            .map_err(|e: num::ParseFloatError| {
                                Diagnostic::at_token(e.to_string(), raw)
                            })?
                            .to_bits(),
                    ),
                    Type::I(int) => Const::Int(int.parse().map_err(|e: num::ParseIntError| {
                        Diagnostic::at_token(e.to_string(), raw)
                    })?),
                    Type::True => Const::True,
                    Type::False => Const::False,
                    _ => unreachable!(),
                };

                let id = self.ctx.id_store.new_value();
                self.emit(Instr::LoadConst {
                    dst: TypeId {
                        id,
                        ty: value.clone().into(),
                    },
                    value,
                    span: raw.start as u32,
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
                    return Err(Diagnostic::at_token(
                        format!("Undefined variable `{i}`"),
                        name,
                    ));
                }
            }
            Node::Field { id, target, name } => {
                let base = self.lower_node(ast, *target)?.unwrap();
                let target_value_id = ast.value_id(*target);
                let target_type = self.types.get(target_value_id).cloned().unwrap();
                let ty = self.types.get(*id).cloned().unwrap();

                let ptype::Type::Record(_) = target_type else {
                    unreachable!();
                };

                let lex::Type::Ident(field_name) = name.t else {
                    unreachable!();
                };
                let offset = target_type
                    .field_offset(field_name)
                    .expect("record field was typechecked") as u32;

                let field_id = self.ctx.id_store.new_value();
                let dst = TypeId { id: field_id, ty };
                // The offset is relative to the target record, but the access
                // mode depends on the field's type: inline nested records are
                // represented by an interior pointer, while scalar fields are
                // loaded as VM values.
                if matches!(dst.ty, ptype::Type::Record(_)) {
                    self.emit(Instr::AddrOf {
                        dst,
                        base,
                        offset,
                        span: name.start as u32,
                    });
                } else {
                    self.emit(Instr::Load {
                        dst,
                        base,
                        offset,
                        span: name.start as u32,
                    });
                }

                Some(field_id)
            }
            Node::Bin { op, lhs, rhs, id } => {
                use BinOp::{
                    BEq, DAdd, DDiv, DGt, DLt, DMul, DSub, IAdd, IDiv, IEq, IGt, ILt, IMod, IMul,
                    ISub, SEq,
                };
                let src_type = self.types.get(ast.value_id(*lhs)).cloned().unwrap();
                let span = op.start as u32;

                let Some(lhs) = self.lower_node(ast, *lhs)? else {
                    unreachable!()
                };
                let Some(rhs) = self.lower_node(ast, *rhs)? else {
                    unreachable!()
                };

                let dst_id = self.ctx.id_store.new_value();
                let dst = TypeId {
                    id: dst_id,
                    ty: self.types.get(*id).cloned().unwrap(),
                };

                let op = match src_type {
                    ptype::Type::Bool => match op.t {
                        Type::DoubleEqual => BEq,
                        _ => unreachable!(),
                    },
                    ptype::Type::Str => match op.t {
                        Type::DoubleEqual => SEq,
                        _ => unreachable!(),
                    },
                    ptype::Type::Int => match op.t {
                        Type::Plus => IAdd,
                        Type::Minus => ISub,
                        Type::Asteriks => IMul,
                        Type::Slash => IDiv,
                        Type::Percent => IMod,
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

                self.emit(Instr::Bin {
                    op,
                    dst,
                    lhs,
                    rhs,
                    span,
                });

                Some(dst_id)
            }
            Node::Unary { op, rhs, .. } => {
                let inner_ty = self.types.get(ast.value_id(*rhs)).cloned().unwrap();
                let span = op.start as u32;
                let Some(rhs_id) = self.lower_node(ast, *rhs)? else {
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
                                ty: inner_ty,
                            },
                            value: zero_const,
                            span,
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
                            span,
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
                if let Some(id) = self.lower_node(ast, *rhs)? {
                    self.ctx.env.insert(i, id);
                } else {
                    return Err(Diagnostic::at_token(
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
                ..
            } => {
                let old_ctx =
                    std::mem::replace(&mut self.ctx, LowerCtx::new_in(self.scratch.clone()));

                let id = Id(self.functions.len() as u32 + 1);
                let Type::Ident(ident_name) = name.t else {
                    unreachable!()
                };

                let ret = if let TypeExpr::Atom(Token { t: Type::Void, .. }) = ast.ty(*return_type)
                {
                    None
                } else if let TypeExpr::Atom(Token { t, .. }) = ast.ty(*return_type) {
                    Some(crate::type_from_lex_type(*t))
                } else {
                    self.types.get(ast.value_id(node_id)).copied()
                };

                self.func_name_to_id.insert(ident_name, (id, ret));
                let func_params: Vec<Id> = args
                    .iter()
                    .map(|(token, _)| {
                        let id = self.ctx.id_store.new_value();
                        let Type::Ident(ident) = token.t else {
                            unreachable!();
                        };
                        self.ctx.env.insert(ident, id);
                        id
                    })
                    .collect();
                let func = Func::new(ident_name, id, func_params, ret).with_span(name.start as u32);

                // TODO:deal with b0

                self.ctx.func = func;
                let entry = self.new_block();
                let entry_params = self.ctx.func.intern_params(self.ctx.func.params.clone());
                self.block_mut(entry).params = entry_params;

                let mut last = None;
                for &node in *body {
                    self.switch_to_block(entry);
                    last = self.lower_node(ast, node)?;
                }

                let ret_span = name.start as u32;
                if self.ctx.func.ret.is_some() {
                    self.block_mut(self.ctx.block).term = Some(Terminator::Return {
                        value: last,
                        span: ret_span,
                    });
                } else {
                    self.block_mut(self.ctx.block).term = Some(Terminator::Return {
                        value: None,
                        span: ret_span,
                    });
                }

                self.functions.push(std::mem::take(&mut self.ctx.func));
                self.ctx = old_ctx;
                None
            }
            Node::Call { target, args, id } => {
                let mut a = vec![];
                for &arg in *args {
                    let Some(id) = self.lower_node(ast, arg)? else {
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

                match ast.node(*target) {
                    // 'syscall' / stdlib call
                    Node::Field { target, name, .. } => {
                        let Node::Ident {
                            name:
                                lex::Token {
                                    t: lex::Type::Ident(pkg_name),
                                    ..
                                },
                            ..
                        } = ast.node(*target)
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
                        let candidates = &self.packages.get(pkg_name).unwrap().1[inner_name];
                        // Single candidate: no specialisation.
                        let fun = if candidates.len() == 1 {
                            candidates[0]
                        } else {
                            // specialisation group: pick the impl matching the arg types with the
                            // same predicate the typechecker used.

                            // arg types stream by reference from the type map; the
                            // typechecker already proved exactly one variant matches.
                            let provided = || {
                                args.iter()
                                    .map(|&n| self.types.get(ast.value_id(n)).unwrap())
                            };
                            *candidates
                                .iter()
                                .find(|f| crate::overload_matches(f.args.iter(), provided()))
                                .unwrap()
                        };

                        // the declared ret may contain generic slots, the typechecker resolved
                        // them per call site
                        dst.ty = self
                            .types
                            .get(*id)
                            .cloned()
                            .expect("typechecker should have typed the call");
                        self.emit(Instr::Sys {
                            dst,
                            path: pkg_name,
                            fun,
                            args: a,
                            span: name.start as u32,
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
                            return Err(Diagnostic::at_token(
                                format!("Undefined function `{inner_name}`"),
                                name,
                            ));
                        };

                        dst.ty = ret.unwrap_or(ptype::Type::Void);
                        self.emit(Instr::Call {
                            dst,
                            func: target_id,
                            args: a,
                            span: name.start as u32,
                        });
                    }
                    _ => unreachable!(),
                }

                Some(dst_id)
            }
            Node::Import { pkgs, .. } => {
                for pkg_tok in *pkgs {
                    let Token {
                        t: Type::S(as_str), ..
                    } = pkg_tok
                    else {
                        unreachable!();
                    };

                    let Some(pkg) = self.resolve_pkg(as_str) else {
                        return Err(Diagnostic::at_token(
                            format!(
                                "Package `{as_str}` was declared by extern signatures but has no runtime implementation"
                            ),
                            pkg_tok,
                        ));
                    };

                    // group specialisations under their group name, mirroring
                    // the typechecker's registration.
                    let mut fns: Overloads<'lower, S> = Map::new_in(self.scratch.clone());
                    for f in pkg.fns {
                        fns.entry(f.group_name())
                            .or_insert_with(|| Vec::new_in(self.scratch.clone()))
                            .push(f);
                    }
                    self.packages.insert(pkg.name, (pkg, fns));
                }
                None
            }
            Node::Extern { .. } => None,
            Node::Cast { id, lhs, src, .. } => {
                let src_ty = self
                    .types
                    .get(ast.value_id(*lhs))
                    .cloned()
                    .expect("typechecker should have typed the cast's lhs");

                let Some(from_id) = self.lower_node(ast, *lhs)? else {
                    unreachable!()
                };

                let dst = self.ctx.id_store.new_value();
                let value = TypeId {
                    id: dst,
                    ty: self
                        .types
                        .get(*id)
                        .copied()
                        .expect("typechecker should have typed the cast"),
                };

                self.emit(Instr::Cast {
                    dst: value,
                    from: TypeId {
                        id: from_id,
                        ty: src_ty,
                    },
                    span: src.start as u32,
                });
                Some(dst)
            }
            Node::Match { cases, default, .. } => {
                let mut check_blocks = Vec::with_capacity_in(cases.len(), self.scratch.clone());
                let mut body_blocks = Vec::with_capacity_in(cases.len(), self.scratch.clone());

                // The first check is lowered into the current block, so the
                // match needs no jump into it.
                let entry = self.ctx.block;
                for i in 0..cases.len() {
                    check_blocks.push(if i == 0 { entry } else { self.new_block() });
                    body_blocks.push(self.new_block());
                }

                // All check/body/default blocks of this match inherit the
                // enclosing block's params
                //
                // Intern that list once and hand the same ParamsId to every sink; 4 * per case,
                // plus the default block, plus the two Branch arms. Each assignment is a u32 copy,
                // no allocation.
                let case_params = {
                    let entry_params = self.cur().params;
                    let cloned: Vec<Id> = self.ctx.func.params(entry_params).to_vec();
                    self.ctx.func.intern_params(cloned)
                };

                let default_block = self.new_block();
                if cases.is_empty() {
                    self.block_mut(entry).term = Some(Terminator::Jump {
                        id: default_block,
                        params: case_params,
                        span: default.0.start as u32,
                    });
                }

                // the single join block, merging all value results into a single branch
                let join = self.new_block();

                for (i, ((case_tok, condition), body)) in cases.iter().enumerate() {
                    let case_span = case_tok.start as u32;
                    self.switch_to_block(check_blocks[i]);
                    let Some(cond) = self.lower_node(ast, *condition)? else {
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
                        yes: (body_blocks[i], case_params),
                        no: (no_target, case_params),
                        span: case_span,
                    });
                    check_block_mut.params = case_params;

                    self.switch_to_block(body_blocks[i]);
                    self.block_mut(body_blocks[i]).params = case_params;

                    // A match arm is its own scope: `let` bindings inside it
                    // (including ones that shadow a param) must not leak into
                    // sibling arms, the default arm, or code after the match.
                    // env is a flat map, so snapshot it and restore afterwards.
                    let saved_env = self.ctx.env.clone();
                    let mut last = None;
                    for &node in *body {
                        last = self.lower_node(ast, node)?;
                    }
                    let value = last.expect("match body must produce value");
                    self.ctx.env = saved_env;

                    let body_jump_params = self.ctx.func.intern_params(vec![value]);
                    self.block_mut(body_blocks[i]).term = Some(Terminator::Jump {
                        id: join,
                        params: body_jump_params,
                        span: case_span,
                    });
                }

                // the typechecker checked we have a default case, so this is safe
                let (default_tok, body) = default;
                let default_span = default_tok.start as u32;
                self.switch_to_block(default_block);
                // Same scoping as the case arms above: the default body's
                // `let` bindings stay local to it.
                let saved_env = self.ctx.env.clone();
                let mut last = None;
                for &node in *body {
                    last = self.lower_node(ast, node)?;
                }

                let last = last.expect("match default must produce value");
                self.ctx.env = saved_env;
                let default_jump_params = self.ctx.func.intern_params(vec![last]);
                let join_params = self.ctx.func.intern_params(vec![last]);
                let default_block_mut = self.block_mut(default_block);
                default_block_mut.params = case_params;
                default_block_mut.term = Some(Terminator::Jump {
                    id: join,
                    params: default_jump_params,
                    span: default_span,
                });

                self.switch_to_block(join);
                self.block_mut(join).params = join_params;
                Some(last)
            }
            Node::Array { id, src, members } => {
                let Some(ty) = self.types.get(*id).cloned() else {
                    unreachable!();
                };

                // we need the inner type T of Array<T> to compute its size and its alignment and
                // multiply it up with the size of the array, since all arrays in pg are immutable
                let ptype::Type::Array(inner) = ty else {
                    unreachable!();
                };

                let inner = *inner;
                let word_size = std::mem::size_of::<purple_garden_runtime::Value>();
                let header_size = align_up(word_size, inner.align());
                let member_size = align_up(inner.size(), inner.align());
                let layout_size = header_size + member_size * members.len();
                let layout_align = word_size.max(inner.align());
                let layout = Layout::from_size_align(layout_size, layout_align)
                    .expect("array allocation layout");

                let array_id = self.ctx.id_store.new_value();
                self.emit(Instr::Alloc {
                    dst: TypeId { id: array_id, ty },
                    layout,
                    span: src.start as u32,
                });

                // PERF: replace this with StoreImm
                let len_id = self.ctx.id_store.new_value();
                self.emit(Instr::LoadConst {
                    dst: TypeId {
                        id: len_id,
                        ty: ptype::Type::Int,
                    },
                    value: i64::try_from(members.len())
                        .expect("Too large of an array, sorry mate")
                        .into(),
                    span: src.start as u32,
                });
                self.emit(Instr::Store {
                    src: len_id,
                    base: array_id,
                    offset: 0,
                    span: src.start as u32,
                });

                for (i, member) in members.iter().enumerate() {
                    let offset = header_size + member_size * i;
                    self.lower_node_into(
                        ast,
                        *member,
                        &inner,
                        array_id,
                        offset as u32,
                        src.start as u32,
                    )?;
                }

                Some(array_id)
            }
            Node::Record { id, src, fields } => {
                let Some(record_ty) = self.types.get(*id).cloned() else {
                    unreachable!();
                };
                let layout = record_ty.layout();
                let id = self.ctx.id_store.new_value();
                self.emit(Instr::Alloc {
                    dst: TypeId { id, ty: record_ty },
                    layout,
                    span: src.start as u32,
                });

                let base = id;
                for (tok, value) in *fields {
                    let lex::Type::Ident(name) = tok.t else {
                        unreachable!();
                    };
                    let offset = record_ty
                        .field_offset(name)
                        .expect("record field was typechecked")
                        as u32;
                    let field_ty = self.types.get(ast.value_id(*value)).cloned().unwrap();
                    self.lower_node_into(ast, *value, &field_ty, base, offset, tok.start as u32)?;
                }

                Some(base)
            }
        })
    }

    /// Lower [ast] using a type map produced by the typechecker.
    ///
    /// The entry point is always `entry`.
    pub fn ir_from_types<B: Allocator>(
        self,
        ast: &'lower Ast<'lower, 'lower>,
        types: TypeMap<'lower, B>,
    ) -> Result<Vec<Func<'lower>>, Diagnostic> {
        Lower {
            scratch: self.scratch,
            ctx: self.ctx,
            functions: self.functions,
            func_name_to_id: self.func_name_to_id,
            types,
            packages: self.packages,
            pkg_cache: self.pkg_cache,
            libs: self.libs,
            stdlib: self.stdlib,
        }
        .lower(ast)
    }

    fn lower(mut self, ast: &'lower Ast<'lower, 'lower>) -> Result<Vec<Func<'lower>>, Diagnostic> {
        // Most roots are declarations or expressions becoming functions later;
        // thus reserving this avoids repeated growth
        self.functions.reserve(ast.roots.len() + 1);
        self.func_name_to_id.reserve(ast.roots.len() + 1);

        self.ctx.func =
            Func::new("entry", Id(0), Vec::new(), None).with_span(ast.entry_span().unwrap_or(0));
        let entry = self.new_block();
        self.switch_to_block(entry);

        let mut last = None;
        let last_span = ast.entry_span().unwrap_or(0);
        for &node in ast.roots {
            last = self.lower_node(ast, node)?;
            // reset to the main entry point block to keep emitting nodes into the correct conext
            self.switch_to_block(entry);
        }

        if last.is_some() {
            self.block_mut(self.ctx.block).term = Some(Terminator::Return {
                value: last,
                span: last_span,
            });
        }

        self.functions.push(self.ctx.func);

        Ok(self.functions)
    }
}
