use crate::lex::{Token, Type};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node<'inner> {
    /// String|Double|Integer|True|False
    Atom { raw: Token<'inner> },

    /// <identifier>
    Ident { name: Token<'inner> },

    /// <lhs> <op> <rhs>
    Bin {
        op: Token<'inner>,
        lhs: Box<Node<'inner>>,
        rhs: Box<Node<'inner>>,
    },

    /// [<member0> <member1>]
    Array { members: Vec<Node<'inner>> },

    /// { <key0>: <value0> <key1>: <value1> }
    Object {
        pairs: Vec<(Node<'inner>, Node<'inner>)>,
    },

    /// let <name> = <rhs>
    Let {
        name: Token<'inner>,
        rhs: Box<Node<'inner>>,
    },

    /// fn <name>(<arg0:type0> <arg1:type1>): <return_type> {
    ///     <body>
    /// }
    Fn {
        name: Token<'inner>,
        /// (<identifier>, <type>)
        args: Vec<(Token<'inner>, TypeExpr<'inner>)>,
        return_type: Option<TypeExpr<'inner>>,
        body: Vec<Node<'inner>>,
    },

    /// match {
    ///    <condition> <body>
    ///    <condition> <body>
    ///    <condition> <body>
    ///    <default>
    /// }
    Match {
        /// [(condition, body)]
        cases: Vec<(Node<'inner>, Node<'inner>)>,
        default: Option<Box<Node<'inner>>>,
    },

    /// <name>(<args>)
    Call {
        name: Token<'inner>,
        args: Vec<Node<'inner>>,
    },

    /// <path0>::<path1>::<leaf>
    Path {
        members: Vec<Token<'inner>>,
        leaf: Box<Node<'inner>>,
    },

    /// <target>[<index>]
    Idx {
        target: Box<Node<'inner>>,
        index: Box<Node<'inner>>,
    },

    /// for <param> :: <target> { <body> }
    For {
        target: Box<Node<'inner>>,
        param: Token<'inner>,
        body: Vec<Node<'inner>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr<'te> {
    /// atom types like: int, str, double, bool
    Atom(Token<'te>),
    /// optionals work via ?<type_expr>
    Option(Box<TypeExpr<'te>>),
    /// Tuple / Array via [<type_expr0> <type_expr1>]
    Tuple(Vec<TypeExpr<'te>>),
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
                if args.is_empty() {
                    write!(f, ")")?;
                } else {
                    for (i, arg) in args.iter().enumerate() {
                        let (Type::Ident(arg_name), type_name) = (&arg.0.t, &arg.1) else {
                            unreachable!();
                        };
                        if i == args.len() - 1 {
                            write!(f, "{}:{:?}", arg_name, type_name)?;
                        } else {
                            write!(f, "{}:{:?} ", arg_name, type_name)?;
                        }
                    }
                    writeln!(f, ")")?;
                }
                for (i, node) in body.iter().enumerate() {
                    node.fmt_sexpr(f, indent + 1)?;
                }
                writeln!(f, "{})->{:?}", pad, return_type)
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
