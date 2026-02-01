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
        /// [(condition, body)]
        cases: Vec<(Node<'node>, Node<'node>)>,
        default: Option<Box<Node<'node>>>,
    },

    /// <name>(<args>)
    Call {
        id: usize,
        name: Token<'node>,
        args: Vec<Node<'node>>,
    },
    // <path0>::<path1>::<leaf>
    // Path {
    //     id: usize,
    //     members: Vec<Token<'node>>,
    //     leaf: Box<Node<'node>>,
    // },

    // <target>[<index>]
    // Idx {
    //     id: usize,
    //     target: Box<Node<'node>>,
    //     index: Box<Node<'node>>,
    // },

    // for <param> :: <target> { <body> }
    // For {
    //     id: usize,
    //     target: Box<Node<'node>>,
    //     param: Token<'node>,
    //     body: Vec<Node<'node>>,
    // },
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
            TypeExpr::Atom(token) => write!(f, "{:?}", token.t),
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
            Node::Atom { raw, .. } => writeln!(f, "{}{:?}", pad, raw.t),
            Node::Ident { name, .. } => {
                if let Type::Ident(name) = name.t {
                    writeln!(f, "{}{}", pad, name)
                } else {
                    unreachable!()
                }
            }
            Node::Bin { op, lhs, rhs, .. } => {
                writeln!(f, "{}({:?}", pad, op.t)?;
                lhs.fmt_sexpr(f, indent + 1)?;
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
                let Type::Ident(name) = name.t else {
                    unreachable!();
                };
                writeln!(f, "{}(let {}", pad, name)?;
                rhs.fmt_sexpr(f, indent + 1)?;
                writeln!(f, "{})", pad)
            }
            Node::Fn {
                name,
                args,
                body,
                return_type,
            } => {
                let Type::Ident(name) = name.t else {
                    unreachable!();
                };
                write!(f, "{}(fn {} (", pad, name)?;
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
            Node::Call { name, args, .. } => {
                let Type::Ident(name) = name.t else {
                    unreachable!();
                };
                write!(f, "{}({}", pad, name)?;
                if !args.is_empty() {
                    writeln!(f)?;
                    for arg in args {
                        arg.fmt_sexpr(f, indent + 1)?;
                    }
                }
                writeln!(f, "{})", pad)
            }
            _ => writeln!(f, "{}<todo {:?}>", pad, self),
        }
    }
}

impl<'a> std::fmt::Display for Node<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_sexpr(f, 0)
    }
}
