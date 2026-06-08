use std::fmt::Display;

use crate::lex::{Token, Type};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeExprId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ast<'ast> {
    pub roots: Vec<NodeId>,
    pub nodes: Vec<Node<'ast>>,
    pub types: Vec<TypeExpr<'ast>>,
}

impl<'ast> Ast<'ast> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_node(&mut self, node: Node<'ast>) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(node);
        id
    }

    pub fn push_type(&mut self, ty: TypeExpr<'ast>) -> TypeExprId {
        let id = TypeExprId(self.types.len());
        self.types.push(ty);
        id
    }

    #[must_use]
    pub fn node(&self, id: NodeId) -> &Node<'ast> {
        &self.nodes[id.0]
    }

    #[must_use]
    pub fn ty(&self, id: TypeExprId) -> &TypeExpr<'ast> {
        &self.types[id.0]
    }

    #[must_use]
    pub fn value_id(&self, id: NodeId) -> Option<usize> {
        self.node(id).value_id()
    }

    #[must_use]
    pub fn entry_span(&self) -> Option<u32> {
        self.roots
            .iter()
            .copied()
            .find(|&node| !matches!(self.node(node), Node::Fn { .. } | Node::Import { .. }))
            .and_then(|node| self.node_start(node))
    }

    #[must_use]
    fn node_start(&self, id: NodeId) -> Option<u32> {
        Some(match self.node(id) {
            Node::Atom { raw, .. } | Node::Ident { name: raw, .. } => raw.start,
            Node::Bin { lhs, op, .. } => self.node_start(*lhs).unwrap_or(op.start as u32) as usize,
            Node::Unary { op, .. } => op.start,
            Node::Array { members, .. } => {
                members.first().and_then(|&node| self.node_start(node))? as usize
            }
            Node::Object { pairs, .. } => {
                pairs.first().and_then(|(key, _)| self.node_start(*key))? as usize
            }
            Node::Let { name, .. } | Node::Fn { name, .. } => name.start,
            Node::Match { cases, default, .. } => cases
                .first()
                .map(|((token, _), _)| token.start)
                .unwrap_or(default.0.start),
            Node::Call { target, .. } | Node::Field { target, .. } => {
                self.node_start(*target)? as usize
            }
            Node::Cast { src, .. } | Node::Import { src, .. } => src.start,
        } as u32)
    }

    #[must_use]
    pub fn type_display(&self, id: TypeExprId) -> TypeDisplay<'_, 'ast> {
        TypeDisplay { ast: self, id }
    }

    #[must_use]
    pub fn type_token(&self, id: TypeExprId) -> &Token<'ast> {
        match self.ty(id) {
            TypeExpr::Atom(token) | TypeExpr::Foreign(token) => token,
            TypeExpr::Option(inner) | TypeExpr::Array(inner) => self.type_token(*inner),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node<'node> {
    /// String|Double|Integer|True|False
    Atom { id: usize, raw: Token<'node> },

    /// <identifier>
    Ident { id: usize, name: Token<'node> },

    /// <lhs> <op> <rhs>
    Bin {
        id: usize,
        op: Token<'node>,
        lhs: NodeId,
        rhs: NodeId,
    },

    /// <op> <rhs>
    Unary {
        id: usize,
        op: Token<'node>,
        rhs: NodeId,
    },

    /// [<member0> <member1>]
    Array { id: usize, members: Vec<NodeId> },

    /// { <key0>: <value0> <key1>: <value1> }
    Object {
        id: usize,
        pairs: Vec<(NodeId, NodeId)>,
    },

    /// let <name> = <rhs>
    Let {
        id: usize,
        name: Token<'node>,
        rhs: NodeId,
    },

    /// fn <name>(<arg0:type0> <arg1:type1>) <`return_type`> {
    ///     <body>
    /// }
    Fn {
        name: Token<'node>,
        /// (<identifier>, <type>)
        args: Vec<(Token<'node>, TypeExprId)>,
        return_type: TypeExprId,
        body: Vec<NodeId>,
    },

    /// match {
    ///    <condition> <body>
    ///    <condition> <body>
    ///    <default>
    /// }
    Match {
        id: usize,
        /// [((`condition_token`, condition), body)]
        cases: Vec<((Token<'node>, NodeId), Vec<NodeId>)>,
        default: (Token<'node>, Vec<NodeId>),
    },

    /// <target>(<args>)
    Call {
        id: usize,
        target: NodeId,
        args: Vec<NodeId>,
    },

    /// <target>.<name>
    Field {
        id: usize,
        target: NodeId,
        name: Token<'node>,
    },

    /// <lhs> as <rhs>
    Cast {
        id: usize,
        src: Token<'node>,
        lhs: NodeId,
        rhs: TypeExprId,
    },

    /// import ("<pkg name>" "<pkg name>")
    Import {
        id: usize,
        src: Token<'node>,
        /// list of packages to import as strings
        pkgs: Vec<Token<'node>>,
    },
}

impl Node<'_> {
    #[must_use]
    fn value_id(&self) -> Option<usize> {
        Some(match self {
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr<'te> {
    /// atom types like: Int, Str, Double, Bool and Void
    Atom(Token<'te>),
    /// foreign types like `Foreign<Counter>`
    Foreign(Token<'te>),
    /// optionals work via `Option<type_expr>`
    Option(TypeExprId),
    /// arrays work via `Array<type_expr>`
    Array(TypeExprId),
}

pub struct TypeDisplay<'ast, 'src> {
    ast: &'ast Ast<'src>,
    id: TypeExprId,
}

impl Display for TypeDisplay<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.ast.ty(self.id) {
            TypeExpr::Atom(token) => write!(f, "{}", token.t.as_str()),
            TypeExpr::Foreign(token) => write!(f, "Foreign<{}>", token.t.as_str()),
            TypeExpr::Option(type_expr) => {
                write!(f, "Option<{}>", self.ast.type_display(*type_expr))
            }
            TypeExpr::Array(type_expr) => write!(f, "Array<{}>", self.ast.type_display(*type_expr)),
        }
    }
}

impl Ast<'_> {
    fn fmt_node_sexpr(
        &self,
        id: NodeId,
        f: &mut std::fmt::Formatter<'_>,
        indent: usize,
    ) -> std::fmt::Result {
        let pad = "  ".repeat(indent);

        match self.node(id) {
            Node::Atom { raw, .. } => match raw.t {
                Type::S(s) => writeln!(f, "{pad}`{s}`"),
                _ => writeln!(f, "{}{}", pad, raw.t.as_str()),
            },
            Node::Ident { name, .. } => {
                if let Type::Ident(name) = name.t {
                    writeln!(f, "{pad}{name}")
                } else {
                    unreachable!()
                }
            }
            Node::Bin { op, lhs, rhs, .. } => {
                writeln!(f, "{}({}", pad, op.t.as_str())?;
                self.fmt_node_sexpr(*lhs, f, indent + 1)?;
                self.fmt_node_sexpr(*rhs, f, indent + 1)?;
                writeln!(f, "{pad})")
            }
            Node::Unary { op, rhs, .. } => {
                writeln!(f, "{}({}", pad, op.t.as_str())?;
                self.fmt_node_sexpr(*rhs, f, indent + 1)?;
                writeln!(f, "{pad})")
            }
            Node::Array { members, .. } => {
                writeln!(f, "{pad}[")?;
                for member in members {
                    self.fmt_node_sexpr(*member, f, indent + 1)?;
                }
                writeln!(f, "{pad}]")
            }
            Node::Object { pairs, .. } => {
                writeln!(f, "{pad}{{")?;
                for (k, v) in pairs {
                    self.fmt_node_sexpr(*k, f, indent + 1)?;
                    self.fmt_node_sexpr(*v, f, indent + 1)?;
                }
                writeln!(f, "{pad}}}")
            }
            Node::Let { name, rhs, .. } => {
                writeln!(f, "{}(let {}", pad, name.t.as_str())?;
                self.fmt_node_sexpr(*rhs, f, indent + 1)?;
                writeln!(f, "{pad})")
            }
            Node::Fn {
                name,
                args,
                body,
                return_type,
            } => {
                write!(f, "{}(fn {} (", pad, name.t.as_str())?;
                for (i, arg) in args.iter().enumerate() {
                    let Type::Ident(arg_name) = arg.0.t else {
                        unreachable!();
                    };
                    if i == args.len() - 1 {
                        write!(f, "{arg_name}:{}", self.type_display(arg.1))?;
                    } else {
                        write!(f, "{arg_name}:{} ", self.type_display(arg.1))?;
                    }
                }
                write!(f, ")")?;
                if !args.is_empty() {
                    writeln!(f)?;
                }
                for node in body {
                    self.fmt_node_sexpr(*node, f, indent + 1)?;
                }
                writeln!(f, "{pad})->{}", self.type_display(*return_type))
            }
            Node::Call { target, args, .. } => {
                write!(f, "{pad}(")?;
                if let Node::Atom {
                    raw: Token { t, .. },
                    ..
                } = self.node(*target)
                {
                    write!(f, "{}", t.as_str())?;
                } else {
                    writeln!(f)?;
                    self.fmt_node_sexpr(*target, f, indent + 1)?;
                }
                for arg in args {
                    self.fmt_node_sexpr(*arg, f, indent + 1)?;
                }
                writeln!(f, "{pad})")
            }
            Node::Cast { lhs, rhs, .. } => {
                let t = crate::type_from_type_expr(self, *rhs);
                writeln!(f, "{pad}(cast_to_{t}")?;
                self.fmt_node_sexpr(*lhs, f, indent + 1)?;
                writeln!(f, "{pad})")
            }
            Node::Match { cases, default, .. } => {
                writeln!(f, "{pad}(match ")?;
                for ((_, condition), body) in cases {
                    writeln!(f, "{pad} (")?;
                    self.fmt_node_sexpr(*condition, f, indent + 1)?;
                    for body_member in body {
                        self.fmt_node_sexpr(*body_member, f, indent + 1)?;
                    }
                    writeln!(f, "{pad} )")?;
                }
                let (_, default) = default;
                writeln!(f, "{pad} (")?;
                for default_member in default {
                    self.fmt_node_sexpr(*default_member, f, indent + 1)?;
                }
                writeln!(f, "{pad} )")?;
                writeln!(f, "{pad})")
            }
            Node::Import { pkgs, .. } => {
                write!(f, "{pad}(import ")?;
                for pkg in pkgs {
                    let Token { t: Type::S(s), .. } = pkg else {
                        unreachable!();
                    };
                    write!(f, "\"{s}\"")?;
                }
                writeln!(f, ")")
            }
            Node::Field { target, name, .. } => {
                writeln!(f, "{pad}(get")?;
                self.fmt_node_sexpr(*target, f, indent + 1)?;
                match name.t {
                    Type::Ident(name) => writeln!(f, "{}  {}", pad, name)?,
                    _ => unreachable!(),
                }
                writeln!(f, "{pad})")
            }
        }
    }
}

impl std::fmt::Display for Ast<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for root in &self.roots {
            self.fmt_node_sexpr(*root, f, 0)?;
        }
        Ok(())
    }
}
