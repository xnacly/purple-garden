use std::fmt::Display;

use crate::lex::{Token, Type};

// TODO: make both Node and TypeExpr allocate into an arena and use indices into said arena instead
// of heap allocating most of the children

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
        lhs: Box<Node<'node>>,
        rhs: Box<Node<'node>>,
    },

    /// <op> <rhs>
    Unary {
        id: usize,
        op: Token<'node>,
        rhs: Box<Node<'node>>,
    },

    /// [<member0> <member1>]
    Array {
        id: usize,
        members: Vec<Node<'node>>,
    },

    /// { <key0>: <value0> <key1>: <value1> }
    Object {
        id: usize,
        pairs: Vec<(Node<'node>, Node<'node>)>,
    },

    /// let <name> = <rhs>
    Let {
        id: usize,
        name: Token<'node>,
        rhs: Box<Node<'node>>,
    },

    /// fn <name>(<arg0:type0> <arg1:type1>) <return_type> {
    ///     <body>
    /// }
    Fn {
        name: Token<'node>,
        /// (<identifier>, <type>)
        args: Vec<(Token<'node>, TypeExpr<'node>)>,
        return_type: TypeExpr<'node>,
        body: Vec<Node<'node>>,
    },

    /// match {
    ///    <condition> <body>
    ///    <condition> <body>
    ///    <condition> <body>
    ///    <default>
    /// }
    Match {
        id: usize,
        /// [((condition_token, condition), body)]
        cases: Vec<((Token<'node>, Node<'node>), Vec<Node<'node>>)>,
        default: (Token<'node>, Vec<Node<'node>>),
    },

    /// <target>(<args>)
    Call {
        id: usize,
        target: Box<Node<'node>>,
        args: Vec<Node<'node>>,
    },

    /// <target>.<name>
    Field {
        id: usize,
        target: Box<Node<'node>>,
        name: Token<'node>,
    },

    /// <lhs> as <rhs>
    Cast {
        id: usize,
        src: Token<'node>,
        lhs: Box<Node<'node>>,
        rhs: TypeExpr<'node>,
    },

    /// import ("<pkg name>" "<pkg name>")
    Import {
        id: usize,
        src: Token<'node>,
        /// list of packages to import as strings
        pkgs: Vec<Token<'node>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr<'te> {
    /// atom types like: int, str, double, bool and void
    Atom(Token<'te>),
    /// optionals work via ?<type_expr>
    Option(Box<TypeExpr<'te>>),
    /// Array via [<type>]
    Array(Box<TypeExpr<'te>>),
    /// Map via <key_type>[<value_type>]
    Map {
        key: Box<TypeExpr<'te>>,
        value: Box<TypeExpr<'te>>,
    },
}

impl Display for TypeExpr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeExpr::Atom(token) => write!(f, "{}", token.t.as_str()),
            TypeExpr::Option(type_expr) => write!(f, "?{}", type_expr),
            TypeExpr::Array(type_expr) => write!(f, "[{}]", type_expr),
            TypeExpr::Map { key, value } => write!(f, "{}[{}]", key, value),
        }
    }
}

impl<'a> Node<'a> {
    fn fmt_sexpr(&self, f: &mut std::fmt::Formatter<'_>, indent: usize) -> std::fmt::Result {
        let pad = "  ".repeat(indent);

        match &self {
            Node::Atom { raw, .. } => writeln!(f, "{}{}", pad, raw.t.as_str()),
            Node::Ident { name, .. } => {
                if let Type::Ident(name) = name.t {
                    writeln!(f, "{}{}", pad, name)
                } else {
                    unreachable!()
                }
            }
            Node::Bin { op, lhs, rhs, .. } => {
                writeln!(f, "{}({}", pad, op.t.as_str())?;
                lhs.fmt_sexpr(f, indent + 1)?;
                rhs.fmt_sexpr(f, indent + 1)?;
                writeln!(f, "{})", pad)
            }
            Node::Unary { op, rhs, .. } => {
                writeln!(f, "{}({}", pad, op.t.as_str())?;
                rhs.fmt_sexpr(f, indent + 1)?;
                writeln!(f, "{})", pad)
            }
            Node::Array { members, .. } => {
                writeln!(f, "{}[", pad)?;
                for member in members {
                    member.fmt_sexpr(f, indent + 1)?;
                }
                writeln!(f, "{}]", pad)
            }
            Node::Object { pairs, .. } => {
                writeln!(f, "{}{{", pad)?;
                for (k, v) in pairs {
                    k.fmt_sexpr(f, indent + 1)?;
                    v.fmt_sexpr(f, indent + 1)?;
                }
                writeln!(f, "{}}}", pad)
            }
            Node::Let { name, rhs, .. } => {
                writeln!(f, "{}(let {}", pad, name.t.as_str())?;
                rhs.fmt_sexpr(f, indent + 1)?;
                writeln!(f, "{})", pad)
            }
            Node::Fn {
                name,
                args,
                body,
                return_type,
            } => {
                write!(f, "{}(fn {} (", pad, name.t.as_str())?;
                for (i, arg) in args.iter().enumerate() {
                    let (Type::Ident(arg_name), type_name) = (&arg.0.t, &arg.1) else {
                        unreachable!();
                    };
                    if i == args.len() - 1 {
                        write!(f, "{}:{}", arg_name, type_name)?;
                    } else {
                        write!(f, "{}:{} ", arg_name, type_name)?;
                    }
                }
                write!(f, ")")?;
                if !args.is_empty() {
                    writeln!(f)?;
                }
                for node in body {
                    node.fmt_sexpr(f, indent + 1)?;
                }
                writeln!(f, "{})->{}", pad, return_type)
            }
            Node::Call { target, args, .. } => {
                write!(f, "{}(", pad,)?;
                target.fmt_sexpr(f, indent);
                if !args.is_empty() {
                    writeln!(f)?;
                    for arg in args {
                        arg.fmt_sexpr(f, indent + 1)?;
                    }
                }
                writeln!(f, "{})", pad)
            }
            Node::Cast { lhs, rhs, .. } => {
                let t: crate::ir::ptype::Type = rhs.into();
                writeln!(f, "{}(cast_to_{}", pad, t)?;
                lhs.fmt_sexpr(f, indent + 1)?;
                writeln!(f, "{})", pad)
            }
            Node::Match { cases, default, .. } => {
                writeln!(f, "{}(match ", pad)?;
                for ((_, condition), body) in cases {
                    writeln!(f, "{} (", pad)?;
                    condition.fmt_sexpr(f, indent + 1)?;
                    for body_member in body {
                        body_member.fmt_sexpr(f, indent + 1)?;
                    }
                    writeln!(f, "{} )", pad)?;
                }
                let (_, default) = default;
                writeln!(f, "{} (", pad)?;
                for default_member in default {
                    default_member.fmt_sexpr(f, indent + 1)?;
                }
                writeln!(f, "{} )", pad)?;
                writeln!(f, "{})", pad)
            }
            Node::Import { pkgs, .. } => {
                write!(f, "{}(import ", pad)?;
                for pkg in pkgs {
                    let Token { t: Type::S(s), .. } = pkg else {
                        unreachable!();
                    };
                    write!(f, "\"{s}\"")?;
                }
                writeln!(f, ")")
            }
            Node::Field { id, target, name } => {
                writeln!(f, "{}(get ", pad)?;
                target.fmt_sexpr(f, indent + 1)?;
                writeln!(f, ".{})", name.t.as_str())
            }
        }
    }
}

impl<'a> std::fmt::Display for Node<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_sexpr(f, 0)
    }
}
