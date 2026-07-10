use std::fmt::Display;

use purple_garden_frontend::{
    ast::{Ast, Node, NodeId},
    lex,
};

use crate::typedefs::{FunctionType, TypecheckOutput};

impl<'t> TypecheckOutput<'t> {
    /// Render top-level binding and function types for `-T`.
    #[must_use]
    pub fn render_summary(&self, ast: &Ast<'t>) -> String {
        let mut out = String::new();
        for &node in &ast.roots {
            match ast.node(node) {
                Node::Let { id, name, .. } => {
                    use std::fmt::Write as _;
                    writeln!(&mut out, "{}: {}", name.t.as_str(), self.type_at(*id)).unwrap();
                }
                Node::Fn {
                    name,
                    args,
                    return_type,
                    ..
                } => {
                    use std::fmt::Write as _;
                    let args = args
                        .iter()
                        .map(|(_, ty)| ast.type_display(*ty).to_string())
                        .collect::<Vec<_>>()
                        .join(" ");
                    writeln!(
                        &mut out,
                        "{}: ({args}) -> {}",
                        name.t.as_str(),
                        ast.type_display(*return_type)
                    )
                    .unwrap();
                }
                _ => {}
            }
        }
        out
    }

    /// Render every typed AST value node for `-TT`.
    #[must_use]
    pub fn render_nodes(&self, ast: &Ast<'t>) -> String {
        let mut out = String::new();
        for &node in &ast.roots {
            self.render_node(ast, node, 0, &mut out);
        }
        out
    }

    fn type_at(&self, id: usize) -> String {
        self.types
            .get(id)
            .and_then(Option::as_ref)
            .map_or_else(|| "<unknown>".to_owned(), ToString::to_string)
    }

    fn render_value(&self, indent: usize, label: impl Display, ty: String, out: &mut String) {
        use std::fmt::Write as _;
        writeln!(out, "{}{}: {ty}", "  ".repeat(indent), label).unwrap();
    }

    fn render_node(&self, ast: &Ast<'t>, node_id: NodeId, indent: usize, out: &mut String) {
        match ast.node(node_id) {
            Node::Record { id, fields, .. } => {
                use std::fmt::Write as _;

                self.render_value(indent, "record", self.type_at(*id), out);
                for (field, value) in fields {
                    let lex::Type::Ident(name) = field.t else {
                        unreachable!()
                    };
                    writeln!(out, "{}field {name}", "  ".repeat(indent + 1)).unwrap();
                    self.render_node(ast, *value, indent + 2, out);
                }
            }
            Node::Atom { id, raw } => {
                self.render_value(indent, raw.t.as_str(), self.type_at(*id), out);
            }
            Node::Ident { id, name } => {
                self.render_value(indent, name.t.as_str(), self.type_at(*id), out);
            }
            Node::Bin { id, op, lhs, rhs } => {
                self.render_value(indent, op.t.as_str(), self.type_at(*id), out);
                self.render_node(ast, *lhs, indent + 1, out);
                self.render_node(ast, *rhs, indent + 1, out);
            }
            Node::Unary { id, op, rhs } => {
                self.render_value(indent, op.t.as_str(), self.type_at(*id), out);
                self.render_node(ast, *rhs, indent + 1, out);
            }
            Node::Array { id, members, .. } => {
                self.render_value(indent, "array", self.type_at(*id), out);
                for &member in members {
                    self.render_node(ast, member, indent + 1, out);
                }
            }
            Node::Let { id, name, rhs, .. } => {
                self.render_value(
                    indent,
                    format!("let {}", name.t.as_str()),
                    self.type_at(*id),
                    out,
                );
                self.render_node(ast, *rhs, indent + 1, out);
            }
            Node::Fn {
                name,
                args,
                return_type,
                body,
                ..
            } => {
                use std::fmt::Write as _;
                let args = args
                    .iter()
                    .map(|(_, ty)| ast.type_display(*ty).to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                writeln!(
                    out,
                    "{}fn {}: ({args}) -> {}",
                    "  ".repeat(indent),
                    name.t.as_str(),
                    ast.type_display(*return_type)
                )
                .unwrap();
                for &node in body {
                    self.render_node(ast, node, indent + 1, out);
                }
            }
            Node::Match { id, cases, default } => {
                self.render_value(indent, "match", self.type_at(*id), out);
                for &((_, condition), ref body) in cases {
                    self.render_node(ast, condition, indent + 1, out);
                    for &node in body {
                        self.render_node(ast, node, indent + 2, out);
                    }
                }
                for &node in &default.1 {
                    self.render_node(ast, node, indent + 1, out);
                }
            }
            Node::Call { id, target, args } => {
                self.render_value(indent, "call", self.type_at(*id), out);
                self.render_callee(ast, *target, indent + 1, out);
                for &arg in args {
                    self.render_node(ast, arg, indent + 1, out);
                }
            }
            Node::Field { id, target, name } => {
                self.render_value(
                    indent,
                    format!(".{}", name.t.as_str()),
                    self.type_at(*id),
                    out,
                );
                self.render_node(ast, *target, indent + 1, out);
            }
            Node::Cast { id, lhs, rhs, .. } => {
                self.render_value(
                    indent,
                    format!("as {}", ast.type_display(*rhs)),
                    self.type_at(*id),
                    out,
                );
                self.render_node(ast, *lhs, indent + 1, out);
            }
            Node::Import { pkgs, .. } => {
                use std::fmt::Write as _;
                for pkg in pkgs {
                    writeln!(out, "{}import {}", "  ".repeat(indent), pkg.t.as_str()).unwrap();
                }
            }
            Node::Extern { name, fns, .. } => {
                use std::fmt::Write as _;
                writeln!(out, "{}extern {}", "  ".repeat(indent), name.t.as_str()).unwrap();
                for fun in fns {
                    let args = fun
                        .args
                        .iter()
                        .map(|(_, ty)| ast.type_display(*ty).to_string())
                        .collect::<Vec<_>>()
                        .join(" ");
                    writeln!(
                        out,
                        "{}fn {}: ({args}) -> {}",
                        "  ".repeat(indent + 1),
                        fun.name.t.as_str(),
                        ast.type_display(fun.return_type)
                    )
                    .unwrap();
                }
            }
        }
    }

    fn render_callee(&self, ast: &Ast<'t>, node_id: NodeId, indent: usize, out: &mut String) {
        use std::fmt::Write as _;
        match ast.node(node_id) {
            Node::Ident { name, .. } => {
                writeln!(out, "{}callee {}", "  ".repeat(indent), name.t.as_str()).unwrap();
            }
            Node::Field { target, name, .. } => match ast.node(*target) {
                Node::Ident { name: pkg, .. } => {
                    writeln!(
                        out,
                        "{}callee {}.{}",
                        "  ".repeat(indent),
                        pkg.t.as_str(),
                        name.t.as_str()
                    )
                    .unwrap();
                }
                _ => self.render_node(ast, node_id, indent, out),
            },
            _ => self.render_node(ast, node_id, indent, out),
        }
    }
}

impl Display for FunctionType<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(")?;
        for (i, (name, t)) in self.args.iter().enumerate() {
            write!(f, "{name}: {t}")?;
            if i + 1 == self.args.len() {
                continue;
            }
            write!(f, " ")?;
        }
        write!(f, ") -> {}", self.ret)?;
        Ok(())
    }
}
