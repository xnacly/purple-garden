use crate::lex::{Token, Type};

#[derive(Debug, Clone)]
pub enum InnerNode<'inner> {
    /// inner value is encoded in super::Node::token
    Atom,
    Ident,

    /// lhs +-*/ rhs
    ///
    /// kind is encoded in super::Node::token
    Bin {
        lhs: Box<Node<'inner>>,
        rhs: Box<Node<'inner>>,
    },

    /// [members]
    Array {
        members: Vec<Node<'inner>>,
    },

    /// { key: value }
    Object {
        pairs: Vec<(Node<'inner>, Node<'inner>)>,
    },

    /// let name = "a string for instance"
    ///
    /// name is encoded in super::Node::token
    Let {
        rhs: Box<Node<'inner>>,
    },

    /// fn square(a) { a * a }
    ///
    /// name is encoded in super::Node::token
    Fn {
        args: Vec<Node<'inner>>,
        body: Vec<Node<'inner>>,
    },

    /// match {
    ///     true && true { false }
    ///     5 == 6 { // impossible }
    ///     5 != 6 { // thats true }
    /// }
    Match {
        /// [(condition, body)]
        cases: Vec<(Node<'inner>, Node<'inner>)>,
        default: Option<Box<Node<'inner>>>,
    },

    /// square(25 5)
    ///
    /// name is encoded in super::Node::token
    Call {
        args: Vec<Node<'inner>>,
    },

    /// std::runtime::gc::cycle()
    Path {
        /// runtime, gc
        members: Vec<Node<'inner>>,
        /// cycle
        ///
        /// always Node::Call, I'd say :^)
        leaf: Box<Node<'inner>>,
    },
}

#[derive(Debug, Clone)]
pub struct Node<'node> {
    pub token: Token<'node>,
    pub inner: InnerNode<'node>,
}

impl<'a> Node<'a> {
    fn fmt_sexpr(&self, f: &mut std::fmt::Formatter<'_>, indent: usize) -> std::fmt::Result {
        let pad = "  ".repeat(indent);

        match &self.inner {
            InnerNode::Atom => writeln!(f, "{}{:?}", pad, self.token.t),
            InnerNode::Ident => {
                if let Type::Ident(ident) = &self.token.t {
                    writeln!(f, "{}{}", pad, ident)
                } else {
                    unreachable!()
                }
            }
            InnerNode::Bin { lhs, rhs } => {
                writeln!(f, "{}({:?}", pad, self.token.t)?;
                lhs.fmt_sexpr(f, indent + 1)?;
                rhs.fmt_sexpr(f, indent + 1)?;
                writeln!(f, "{})", pad)
            }
            InnerNode::Array { members } => {
                writeln!(f, "{}[", pad)?;
                for member in members {
                    member.fmt_sexpr(f, indent + 1)?;
                }
                writeln!(f, "{}]", pad)
            }
            InnerNode::Object { pairs } => {
                writeln!(f, "{}{{", pad)?;
                for (k, v) in pairs {
                    k.fmt_sexpr(f, indent + 1)?;
                    v.fmt_sexpr(f, indent + 1)?;
                }
                writeln!(f, "{}}}", pad)
            }
            InnerNode::Let { rhs } => {
                let Type::Ident(name) = self.token.t else {
                    unreachable!();
                };
                writeln!(f, "{}(let {}", pad, name)?;
                rhs.fmt_sexpr(f, indent + 1)?;
                writeln!(f, "{})", pad)
            }
            InnerNode::Fn { args, body } => {
                let Type::Ident(name) = self.token.t else {
                    unreachable!();
                };
                write!(f, "{}(fn {} (", pad, name)?;
                if args.is_empty() {
                    write!(f, ")")?;
                } else {
                    for (i, arg) in args.iter().enumerate() {
                        let Type::Ident(arg_name) = arg.token.t else {
                            unreachable!();
                        };
                        if i == args.len() - 1 {
                            write!(f, "{}", arg_name)?;
                        } else {
                            write!(f, "{} ", arg_name)?;
                        }
                    }
                    writeln!(f, ")")?;
                }
                for (i, node) in body.iter().enumerate() {
                    node.fmt_sexpr(f, indent + 1)?;
                }
                writeln!(f, "{})", pad)
            }
            InnerNode::Call { args } => {
                let Type::Ident(name) = self.token.t else {
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
            _ => writeln!(f, "{}<todo {:?}>", pad, self.inner),
        }
    }
}

impl<'a> std::fmt::Display for Node<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_sexpr(f, 0)
    }
}
