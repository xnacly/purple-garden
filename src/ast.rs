use crate::lex::{Token, Type};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node<'inner> {
    /// inner value is encoded in super::Node::token
    Atom {
        raw: Token<'inner>,
    },
    Ident {
        name: Token<'inner>,
    },

    /// lhs +-*/ rhs
    ///
    /// kind is encoded in super::Node::token
    Bin {
        op: Token<'inner>,
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
        name: Token<'inner>,
        rhs: Box<Node<'inner>>,
    },

    /// fn square(a) { a * a }
    ///
    /// name is encoded in super::Node::token
    Fn {
        name: Token<'inner>,
        args: Vec<Token<'inner>>,
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
        name: Token<'inner>,
        args: Vec<Node<'inner>>,
    },

    /// std::runtime::gc::cycle()
    Path {
        /// runtime, gc
        members: Vec<Token<'inner>>,
        /// cycle
        ///
        /// always Node::Call, I'd say :^)
        leaf: Box<Node<'inner>>,
    },

    Idx {
        target: Box<Node<'inner>>,
        index: Box<Node<'inner>>,
    },
}

impl<'a> Node<'a> {
    fn fmt_sexpr(&self, f: &mut std::fmt::Formatter<'_>, indent: usize) -> std::fmt::Result {
        let pad = "  ".repeat(indent);

        match &self {
            Node::Atom { raw } => writeln!(f, "{}{:?}", pad, raw),
            Node::Ident { name } => {
                if let Type::Ident(name) = name.t {
                    writeln!(f, "{}{}", pad, name)
                } else {
                    unreachable!()
                }
            }
            Node::Bin { op, lhs, rhs } => {
                writeln!(f, "{}({:?}", pad, op)?;
                lhs.fmt_sexpr(f, indent + 1)?;
                rhs.fmt_sexpr(f, indent + 1)?;
                writeln!(f, "{})", pad)
            }
            Node::Array { members } => {
                writeln!(f, "{}[", pad)?;
                for member in members {
                    member.fmt_sexpr(f, indent + 1)?;
                }
                writeln!(f, "{}]", pad)
            }
            Node::Object { pairs } => {
                writeln!(f, "{}{{", pad)?;
                for (k, v) in pairs {
                    k.fmt_sexpr(f, indent + 1)?;
                    v.fmt_sexpr(f, indent + 1)?;
                }
                writeln!(f, "{}}}", pad)
            }
            Node::Let { name, rhs } => {
                let Type::Ident(name) = name.t else {
                    unreachable!();
                };
                writeln!(f, "{}(let {}", pad, name)?;
                rhs.fmt_sexpr(f, indent + 1)?;
                writeln!(f, "{})", pad)
            }
            Node::Fn { name, args, body } => {
                let Type::Ident(name) = name.t else {
                    unreachable!();
                };
                write!(f, "{}(fn {} (", pad, name)?;
                if args.is_empty() {
                    write!(f, ")")?;
                } else {
                    for (i, arg) in args.iter().enumerate() {
                        let Type::Ident(arg_name) = arg.t else {
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
            Node::Call { name, args } => {
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
