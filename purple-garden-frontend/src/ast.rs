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
            .find(|&node| {
                !matches!(
                    self.node(node),
                    Node::Fn { .. } | Node::Import { .. } | Node::Extern { .. }
                )
            })
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
            Node::Cast { src, .. }
            | Node::Import { src, .. }
            | Node::Extern { src, .. }
            | Node::Record { src, .. } => src.start,
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
            TypeExpr::Record { src, .. } => src,
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
        docs: Vec<Token<'node>>,
        name: Token<'node>,
        rhs: NodeId,
    },

    /// fn <name>(<arg0:type0> <arg1:type1>) <`return_type`> {
    ///     <body>
    /// }
    Fn {
        docs: Vec<Token<'node>>,
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

    /// extern "<pkg name>" { fn <name>(<arg0:type0>) <return_type> }
    Extern {
        src: Token<'node>,
        docs: Vec<Token<'node>>,
        name: Token<'node>,
        fns: Vec<ExternFn<'node>>,
    },

    /// { <field> <value> }
    Record {
        id: usize,
        src: Token<'node>,
        fields: Vec<(Token<'node>, NodeId)>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternFn<'node> {
    pub docs: Vec<Token<'node>>,
    pub name: Token<'node>,
    pub args: Vec<(Token<'node>, TypeExprId)>,
    pub return_type: TypeExprId,
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
            | Node::Record { id, .. }
            | Node::Field { id, .. } => *id,
            Node::Fn { .. } | Node::Import { .. } | Node::Extern { .. } => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr<'te> {
    Atom(Token<'te>),
    Foreign(Token<'te>),
    Option(TypeExprId),
    Array(TypeExprId),
    Record {
        src: Token<'te>,
        fields: Vec<(Token<'te>, TypeExprId)>,
    },
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
            TypeExpr::Record { fields, .. } => {
                if fields.is_empty() {
                    write!(f, "Record<>")
                } else {
                    write!(f, "Record<")?;
                    for (i, (key, value)) in fields.iter().enumerate() {
                        if i > 0 {
                            write!(f, " ")?;
                        }
                        write!(f, "{}: {}", key.t.as_str(), self.ast.type_display(*value))?;
                    }
                    write!(f, ">")
                }
            }
        }
    }
}

impl Ast<'_> {
    fn fmt_ident_token<'src>(token: &Token<'src>) -> &'src str {
        let Type::Ident(name) = token.t else {
            unreachable!();
        };
        name
    }

    fn fmt_arg_list(
        &self,
        f: &mut std::fmt::Formatter<'_>,
        args: &[(Token<'_>, TypeExprId)],
    ) -> std::fmt::Result {
        write!(f, "(")?;
        for (i, (name, ty)) in args.iter().enumerate() {
            if i > 0 {
                write!(f, " ")?;
            }
            write!(
                f,
                "{}: {}",
                Self::fmt_ident_token(name),
                self.type_display(*ty)
            )?;
        }
        write!(f, ")")
    }

    fn fmt_node_inline(&self, id: NodeId, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.node(id) {
            Node::Record { fields, .. } => {
                write!(f, "(record")?;
                for (name, value) in fields {
                    write!(f, " {}: ", Self::fmt_ident_token(name))?;
                    self.fmt_node_inline(*value, f)?;
                }
                write!(f, ")")
            }
            Node::Atom { raw, .. } => match raw.t {
                Type::S(s) => write!(f, "\"{s}\""),
                _ => write!(f, "{}", raw.t.as_str()),
            },
            Node::Ident { name, .. } => write!(f, "{}", Self::fmt_ident_token(name)),
            Node::Bin { op, lhs, rhs, .. } => {
                write!(f, "({} ", op.t.as_str())?;
                self.fmt_node_inline(*lhs, f)?;
                write!(f, " ")?;
                self.fmt_node_inline(*rhs, f)?;
                write!(f, ")")
            }
            Node::Unary { op, rhs, .. } => {
                write!(f, "({} ", op.t.as_str())?;
                self.fmt_node_inline(*rhs, f)?;
                write!(f, ")")
            }
            Node::Array { members, .. } => {
                write!(f, "(array")?;
                for member in members {
                    write!(f, " ")?;
                    self.fmt_node_inline(*member, f)?;
                }
                write!(f, ")")
            }
            Node::Call { target, args, .. } => {
                write!(f, "(call target: ")?;
                self.fmt_node_inline(*target, f)?;
                for arg in args {
                    write!(f, " arg: ")?;
                    self.fmt_node_inline(*arg, f)?;
                }
                write!(f, ")")
            }
            Node::Cast { lhs, rhs, .. } => {
                write!(f, "(cast {} ", self.type_display(*rhs))?;
                self.fmt_node_inline(*lhs, f)?;
                write!(f, ")")
            }
            Node::Field { target, name, .. } => {
                write!(f, "(field target: ")?;
                self.fmt_node_inline(*target, f)?;
                write!(f, " name: {})", Self::fmt_ident_token(name))
            }
            Node::Let { name, rhs, .. } => {
                write!(f, "(let {} ", Self::fmt_ident_token(name))?;
                self.fmt_node_inline(*rhs, f)?;
                write!(f, ")")
            }
            Node::Object { pairs, .. } => {
                write!(f, "(object")?;
                for (key, value) in pairs {
                    write!(f, " key: ")?;
                    self.fmt_node_inline(*key, f)?;
                    write!(f, " value: ")?;
                    self.fmt_node_inline(*value, f)?;
                }
                write!(f, ")")
            }
            Node::Match { .. } | Node::Fn { .. } | Node::Import { .. } | Node::Extern { .. } => {
                unreachable!("record values are expressions")
            }
        }
    }

    fn fmt_record_fields(
        &self,
        fields: &[(Token<'_>, NodeId)],
        f: &mut std::fmt::Formatter<'_>,
        indent: usize,
    ) -> std::fmt::Result {
        let pad = "  ".repeat(indent);

        for (name, value) in fields {
            write!(f, "{}{}: ", pad, Self::fmt_ident_token(name))?;
            if let Node::Record { fields, .. } = self.node(*value) {
                if fields.is_empty() {
                    writeln!(f, "(record)")?;
                } else {
                    writeln!(f, "(record")?;
                    self.fmt_record_fields(fields, f, indent + 1)?;
                    writeln!(f, "{pad})")?;
                }
            } else {
                self.fmt_node_inline(*value, f)?;
                writeln!(f)?;
            }
        }

        Ok(())
    }

    fn fmt_node_sexpr(
        &self,
        id: NodeId,
        f: &mut std::fmt::Formatter<'_>,
        indent: usize,
    ) -> std::fmt::Result {
        let pad = "  ".repeat(indent);

        match self.node(id) {
            Node::Record { .. } => {
                let Node::Record { fields, .. } = self.node(id) else {
                    unreachable!();
                };

                if fields.is_empty() {
                    return writeln!(f, "{pad}(record)");
                }

                writeln!(f, "{pad}(record")?;
                self.fmt_record_fields(fields, f, indent + 1)?;
                writeln!(f, "{pad})")
            }
            Node::Atom { raw, .. } => match raw.t {
                Type::S(s) => writeln!(f, "{pad}\"{s}\""),
                _ => writeln!(f, "{}{}", pad, raw.t.as_str()),
            },
            Node::Ident { name, .. } => writeln!(f, "{}{}", pad, Self::fmt_ident_token(name)),
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
                writeln!(f, "{pad}(array")?;
                for member in members {
                    self.fmt_node_sexpr(*member, f, indent + 1)?;
                }
                writeln!(f, "{pad})")
            }
            Node::Object { pairs, .. } => {
                let child_pad = "  ".repeat(indent + 1);

                writeln!(f, "{pad}(object")?;
                for (k, v) in pairs {
                    writeln!(f, "{child_pad}key:")?;
                    self.fmt_node_sexpr(*k, f, indent + 2)?;
                    writeln!(f, "{child_pad}value:")?;
                    self.fmt_node_sexpr(*v, f, indent + 2)?;
                }
                writeln!(f, "{pad})")
            }
            Node::Let { name, rhs, .. } => {
                writeln!(f, "{}(let {}", pad, Self::fmt_ident_token(name))?;
                self.fmt_node_sexpr(*rhs, f, indent + 1)?;
                writeln!(f, "{pad})")
            }
            Node::Fn {
                name,
                args,
                body,
                return_type,
                ..
            } => {
                write!(f, "{}(fn {} ", pad, Self::fmt_ident_token(name))?;
                self.fmt_arg_list(f, args)?;
                writeln!(f, " -> {}", self.type_display(*return_type))?;
                for node in body {
                    self.fmt_node_sexpr(*node, f, indent + 1)?;
                }
                writeln!(f, "{pad})")
            }
            Node::Call { target, args, .. } => {
                let child_pad = "  ".repeat(indent + 1);

                writeln!(f, "{pad}(call")?;
                writeln!(f, "{child_pad}target:")?;
                self.fmt_node_sexpr(*target, f, indent + 2)?;
                for arg in args {
                    writeln!(f, "{child_pad}arg:")?;
                    self.fmt_node_sexpr(*arg, f, indent + 2)?;
                }
                writeln!(f, "{pad})")
            }
            Node::Cast { lhs, rhs, .. } => {
                writeln!(f, "{}(cast {}", pad, self.type_display(*rhs))?;
                self.fmt_node_sexpr(*lhs, f, indent + 1)?;
                writeln!(f, "{pad})")
            }
            Node::Match { cases, default, .. } => {
                let case_pad = "  ".repeat(indent + 1);
                let label_pad = "  ".repeat(indent + 2);

                writeln!(f, "{pad}(match")?;
                for ((_, condition), body) in cases {
                    writeln!(f, "{case_pad}(case")?;
                    writeln!(f, "{label_pad}when:")?;
                    self.fmt_node_sexpr(*condition, f, indent + 3)?;
                    writeln!(f, "{label_pad}then:")?;
                    for body_member in body {
                        self.fmt_node_sexpr(*body_member, f, indent + 3)?;
                    }
                    writeln!(f, "{case_pad})")?;
                }
                let (_, default) = default;
                writeln!(f, "{case_pad}(default")?;
                for default_member in default {
                    self.fmt_node_sexpr(*default_member, f, indent + 2)?;
                }
                writeln!(f, "{case_pad})")?;
                writeln!(f, "{pad})")
            }
            Node::Import { pkgs, .. } => {
                write!(f, "{pad}(import")?;
                for pkg in pkgs {
                    let Token { t: Type::S(s), .. } = pkg else {
                        unreachable!();
                    };
                    write!(f, " \"{s}\"")?;
                }
                writeln!(f, ")")
            }
            Node::Extern { name, fns, .. } => {
                let child_pad = "  ".repeat(indent + 1);

                writeln!(f, "{}(extern \"{}\"", pad, name.t.as_str())?;
                for fun in fns {
                    write!(f, "{}(fn {} ", child_pad, Self::fmt_ident_token(&fun.name))?;
                    self.fmt_arg_list(f, &fun.args)?;
                    writeln!(f, " -> {})", self.type_display(fun.return_type))?;
                }
                writeln!(f, "{pad})")
            }
            Node::Field { target, name, .. } => {
                let child_pad = "  ".repeat(indent + 1);

                writeln!(f, "{pad}(field")?;
                writeln!(f, "{child_pad}target:")?;
                self.fmt_node_sexpr(*target, f, indent + 2)?;
                writeln!(f, "{}name: {}", child_pad, Self::fmt_ident_token(name))?;
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
