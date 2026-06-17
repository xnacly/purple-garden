use std::collections::HashMap;

use purple_garden_frontend::{
    ast::{Ast, Node, NodeId},
    diagnostic::Span,
    lex::Token,
};

use super::{analysis::DefinitionEntry, source::token_span};

#[derive(Debug, Default)]
pub(super) struct DefinitionCollector<'src> {
    scopes: Vec<HashMap<&'src str, Span>>,
    functions: HashMap<&'src str, Span>,
    imports: HashMap<&'src str, Span>,
    entries: Vec<DefinitionEntry>,
}

impl<'src> DefinitionCollector<'src> {
    pub(super) fn collect(ast: &Ast<'src>) -> Vec<DefinitionEntry> {
        let mut collector = Self::default();
        collector.scopes.push(HashMap::new());

        for &root in &ast.roots {
            match ast.node(root) {
                Node::Fn { name, .. } => {
                    collector
                        .functions
                        .insert(name.t.as_str(), token_span(name));
                }
                Node::Import { pkgs, .. } => {
                    for pkg in pkgs {
                        collector.imports.insert(pkg.t.as_str(), token_span(pkg));
                    }
                }
                Node::Extern { name, .. } => {
                    collector.imports.insert(name.t.as_str(), token_span(name));
                }
                _ => {}
            }
        }

        for &root in &ast.roots {
            collector.node(ast, root);
        }
        collector.entries
    }

    fn node(&mut self, ast: &Ast<'src>, node_id: NodeId) {
        match ast.node(node_id) {
            Node::Atom { .. } => {}
            Node::Ident { name, .. } => self.ident(name),
            Node::Bin { lhs, rhs, .. } => {
                self.node(ast, *lhs);
                self.node(ast, *rhs);
            }
            Node::Unary { rhs, .. } => self.node(ast, *rhs),
            Node::Array { members, .. } => {
                for &member in members {
                    self.node(ast, member);
                }
            }
            Node::Object { pairs, .. } => {
                for &(key, value) in pairs {
                    self.node(ast, key);
                    self.node(ast, value);
                }
            }
            Node::Let { name, rhs, .. } => {
                self.node(ast, *rhs);
                self.insert_local(name);
            }
            Node::Fn {
                name, args, body, ..
            } => {
                self.add_definition(name, token_span(name));
                self.scopes.push(HashMap::new());
                for (arg, _) in args {
                    self.insert_local(arg);
                }
                for &node in body {
                    self.node(ast, node);
                }
                self.scopes.pop();
            }
            Node::Match { cases, default, .. } => {
                for &((_, condition), ref body) in cases {
                    self.node(ast, condition);
                    self.scopes.push(HashMap::new());
                    for &node in body {
                        self.node(ast, node);
                    }
                    self.scopes.pop();
                }
                self.scopes.push(HashMap::new());
                for &node in &default.1 {
                    self.node(ast, node);
                }
                self.scopes.pop();
            }
            Node::Call { target, args, .. } => {
                self.node(ast, *target);
                for &arg in args {
                    self.node(ast, arg);
                }
            }
            Node::Field { target, .. } => self.node(ast, *target),
            Node::Cast { lhs, .. } => self.node(ast, *lhs),
            Node::Import { pkgs, .. } => {
                for pkg in pkgs {
                    self.add_definition(pkg, token_span(pkg));
                }
            }
            Node::Extern { name, .. } => {
                self.add_definition(name, token_span(name));
            }
        }
    }

    fn ident(&mut self, token: &Token<'src>) {
        let name = token.t.as_str();
        if let Some(definition) = self
            .lookup_local(name)
            .or_else(|| self.functions.get(name).copied())
            .or_else(|| self.imports.get(name).copied())
        {
            self.add_definition(token, definition);
        }
    }

    fn insert_local(&mut self, token: &Token<'src>) {
        let span = token_span(token);
        self.scopes
            .last_mut()
            .expect("definition collector has a scope")
            .insert(token.t.as_str(), span);
        self.add_definition(token, span);
    }

    fn lookup_local(&self, name: &str) -> Option<Span> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    fn add_definition(&mut self, token: &Token<'src>, definition: Span) {
        self.entries.push(DefinitionEntry {
            reference: token_span(token),
            definition,
        });
    }
}
