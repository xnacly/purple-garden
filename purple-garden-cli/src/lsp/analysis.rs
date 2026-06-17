use lsp_types::{
    CompletionItem, CompletionItemKind, Hover, HoverContents, MarkupContent, MarkupKind, Position,
};
use purple_garden_frontend::{
    ast::{Ast, Node, NodeId, TypeExprId},
    diagnostic::{Diagnostic as FrontendDiagnostic, Span},
    lex::{Lexer, Token, Type},
    parser::Parser,
    typecheck::{TypecheckOutput, Typechecker},
};

use super::{
    completion::{self, CompletionEntry},
    diagnostic::diagnostic_for_lsp,
    source::{
        node_span, offset_for_position, range_for_span, span_contains, token_span, type_expr_span,
    },
};

#[derive(Default)]
pub(super) struct DocumentState {
    text: String,
    analysis: DocumentAnalysis,
}

#[derive(Debug, Clone, Default)]
struct DocumentAnalysis {
    diagnostics: Vec<FrontendDiagnostic>,
    hovers: Vec<HoverEntry>,
    completions: Vec<CompletionEntry>,
}

#[derive(Debug, Clone)]
struct HoverEntry {
    span: Span,
    contents: String,
}

impl DocumentState {
    pub(super) fn analyze(text: String) -> Self {
        let parse = Parser::new(Lexer::new(text.as_bytes())).parse_collect();
        let purple_garden_frontend::parser::ParseOutput {
            ast,
            mut diagnostics,
        } = parse;

        let mut analysis = DocumentAnalysis::new();
        collect_lexical_hovers(&text, &mut analysis);
        if let Some(ast) = ast {
            let typecheck = Typechecker::new(&ast).check();
            for &root in &ast.roots {
                collect_analysis_entries(&ast, &typecheck, root, &mut analysis);
            }
            diagnostics.extend(typecheck.diagnostics);
        }
        analysis.diagnostics = diagnostics;

        Self { text, analysis }
    }

    pub(super) fn diagnostics(&self) -> Vec<lsp_types::Diagnostic> {
        self.analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic_for_lsp(diagnostic, &self.text))
            .collect()
    }

    pub(super) fn code_actions(
        &self,
        uri: lsp_types::Uri,
        range: lsp_types::Range,
    ) -> lsp_types::CodeActionResponse {
        super::code_action::actions_for(uri, &self.text, &self.analysis.diagnostics, range)
    }

    pub(super) fn hover_at(&self, position: Position) -> Option<Hover> {
        let offset = offset_for_position(&self.text, position);
        let entry = self
            .analysis
            .hovers
            .iter()
            .filter(|entry| span_contains(entry.span, offset))
            .min_by_key(|entry| entry.span.len)?;

        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: entry.contents.clone(),
            }),
            range: Some(range_for_span(&self.text, entry.span)),
        })
    }

    pub(super) fn completions_at(&self, position: Position) -> Vec<CompletionItem> {
        let offset = offset_for_position(&self.text, position);
        completion::items_at(&self.analysis.completions, &self.text, offset)
    }
}

impl DocumentAnalysis {
    fn new() -> Self {
        Self {
            completions: completion::global_completions().to_vec(),
            ..Default::default()
        }
    }

    fn add_garden_hover(&mut self, span: Span, contents: impl Into<String>) {
        self.hovers.push(HoverEntry {
            span,
            contents: garden_block(contents.into()),
        });
    }

    fn add_markdown_hover(&mut self, span: Span, contents: impl Into<String>) {
        self.hovers.push(HoverEntry {
            span,
            contents: contents.into(),
        });
    }

    fn add_hover(&mut self, hover: AnalysisHover) {
        match hover.markup {
            HoverMarkup::Garden(contents) => self.add_garden_hover(hover.span, contents),
            HoverMarkup::Markdown(contents) => self.add_markdown_hover(hover.span, contents),
        }
    }

    fn add_completion(
        &mut self,
        label: impl Into<String>,
        kind: CompletionItemKind,
        detail: Option<String>,
    ) {
        self.completions
            .push(CompletionEntry::local(label, kind, detail));
    }
}

fn collect_analysis_entries(
    ast: &Ast<'_>,
    typecheck: &TypecheckOutput<'_>,
    node_id: NodeId,
    analysis: &mut DocumentAnalysis,
) {
    let node = ast.node(node_id);
    if let (Some(span), Some(ty)) = (
        node_span(ast, node_id),
        type_for_node(ast, typecheck, node_id),
    ) {
        analysis.add_garden_hover(span, ty);
    }

    match node {
        Node::Fn {
            name,
            args,
            return_type,
            body,
        } => {
            let detail = fn_detail(ast, name, args, *return_type);
            analysis.add_garden_hover(token_span(name), detail.clone());
            analysis.add_completion(name.t.as_str(), CompletionItemKind::FUNCTION, Some(detail));
            for (name, ty) in args {
                let detail = format!("{}: {}", name.t.as_str(), ast.type_display(*ty));
                analysis.add_garden_hover(token_span(name), detail.clone());
                analysis.add_completion(
                    name.t.as_str(),
                    CompletionItemKind::VARIABLE,
                    Some(detail),
                );
            }
            collect_nodes(ast, typecheck, body, analysis);
        }
        Node::Let { name, rhs, .. } => {
            if let Some(ty) = type_for_node(ast, typecheck, *rhs) {
                let detail = format!("{}: {}", name.t.as_str(), ty);
                analysis.add_garden_hover(token_span(name), detail.clone());
                analysis.add_completion(
                    name.t.as_str(),
                    CompletionItemKind::VARIABLE,
                    Some(detail),
                );
            }
            collect_analysis_entries(ast, typecheck, *rhs, analysis);
        }
        Node::Field { target, name, .. } => {
            if let Some((span, detail)) = package_target_hover(ast, *target) {
                analysis.add_markdown_hover(span, detail);
            }
            if let Some(ty) = type_for_node(ast, typecheck, node_id) {
                analysis
                    .add_garden_hover(token_span(name), format!(".{}: {}", name.t.as_str(), ty));
            }
            collect_analysis_entries(ast, typecheck, *target, analysis);
        }
        Node::Cast { lhs, rhs, .. } => {
            collect_analysis_entries(ast, typecheck, *lhs, analysis);
            analysis.add_garden_hover(
                type_expr_span(ast, *rhs),
                ast.type_display(*rhs).to_string(),
            );
        }
        Node::Bin { lhs, rhs, .. } => collect_nodes(ast, typecheck, &[*lhs, *rhs], analysis),
        Node::Unary { rhs, .. } => collect_analysis_entries(ast, typecheck, *rhs, analysis),
        Node::Array { members, .. } => collect_nodes(ast, typecheck, members, analysis),
        Node::Object { pairs, .. } => {
            for &(key, value) in pairs {
                collect_analysis_entries(ast, typecheck, key, analysis);
                collect_analysis_entries(ast, typecheck, value, analysis);
            }
        }
        Node::Match { cases, default, .. } => {
            for &((_, condition), ref body) in cases {
                collect_analysis_entries(ast, typecheck, condition, analysis);
                collect_nodes(ast, typecheck, body, analysis);
            }
            collect_nodes(ast, typecheck, &default.1, analysis);
        }
        Node::Call { target, args, .. } => {
            if let Some(hover) = call_hover(ast, *target) {
                analysis.add_hover(hover);
            }
            collect_analysis_entries(ast, typecheck, *target, analysis);
            collect_nodes(ast, typecheck, args, analysis);
        }
        Node::Import { pkgs, .. } => {
            for pkg in pkgs {
                if let Some(detail) = import_hover(pkg) {
                    analysis.add_markdown_hover(token_span(pkg), detail);
                }
            }
        }
        Node::Atom { .. } | Node::Ident { .. } => {}
    }
}

fn collect_nodes(
    ast: &Ast<'_>,
    typecheck: &TypecheckOutput<'_>,
    nodes: &[NodeId],
    analysis: &mut DocumentAnalysis,
) {
    for &node in nodes {
        collect_analysis_entries(ast, typecheck, node, analysis);
    }
}

fn collect_lexical_hovers(source: &str, analysis: &mut DocumentAnalysis) {
    let mut lexer = Lexer::new(source.as_bytes());
    loop {
        let token = lexer.one();
        if token.t == Type::Eof {
            break;
        }

        let Some((kind, query)) = language_doc_query(&token) else {
            continue;
        };
        match kind {
            LanguageDocKind::Keyword => {
                let Some(doc) = crate::doc::language_doc(query) else {
                    continue;
                };
                analysis.add_markdown_hover(
                    token_span(&token),
                    format!(
                        "{} {}\n\n{}\n\n{}",
                        doc.kind,
                        doc.name,
                        doc.doc,
                        crate::doc::command(query)
                    ),
                );
            }
            LanguageDocKind::Type => {
                let Some(doc) = crate::doc::type_doc(query) else {
                    continue;
                };
                analysis.add_markdown_hover(
                    token_span(&token),
                    format!(
                        "type {}\n\n{}\n\n{}",
                        doc.name,
                        doc.doc,
                        crate::doc::command(query)
                    ),
                );
            }
        }
    }
}

enum LanguageDocKind {
    Keyword,
    Type,
}

fn language_doc_query(token: &Token<'_>) -> Option<(LanguageDocKind, &'static str)> {
    Some(match token.t {
        Type::Import => (LanguageDocKind::Keyword, "import"),
        Type::Let => (LanguageDocKind::Keyword, "let"),
        Type::Fn => (LanguageDocKind::Keyword, "fn"),
        Type::Match => (LanguageDocKind::Keyword, "match"),
        Type::As => (LanguageDocKind::Keyword, "as"),
        Type::True => (LanguageDocKind::Keyword, "true"),
        Type::False => (LanguageDocKind::Keyword, "false"),
        Type::Str => (LanguageDocKind::Type, "Str"),
        Type::Int => (LanguageDocKind::Type, "Int"),
        Type::Double => (LanguageDocKind::Type, "Double"),
        Type::Bool => (LanguageDocKind::Type, "Bool"),
        Type::Void => (LanguageDocKind::Type, "Void"),
        _ => return None,
    })
}

fn fn_detail(
    ast: &Ast<'_>,
    name: &Token<'_>,
    args: &[(Token<'_>, TypeExprId)],
    return_type: TypeExprId,
) -> String {
    let args = args
        .iter()
        .map(|(name, ty)| format!("{}: {}", name.t.as_str(), ast.type_display(*ty)))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "fn {}({}) {}",
        name.t.as_str(),
        args,
        ast.type_display(return_type)
    )
}

struct AnalysisHover {
    span: Span,
    markup: HoverMarkup,
}

enum HoverMarkup {
    Garden(String),
    Markdown(String),
}

fn call_hover(ast: &Ast<'_>, target: NodeId) -> Option<AnalysisHover> {
    match ast.node(target) {
        Node::Ident { name, .. } => {
            local_function_detail(ast, name.t.as_str()).map(|detail| AnalysisHover {
                span: token_span(name),
                markup: HoverMarkup::Garden(detail),
            })
        }
        Node::Field { target, name, .. } => {
            let Node::Ident { name: pkg, .. } = ast.node(*target) else {
                return None;
            };
            let pkg_name = pkg.t.as_str();
            let fn_name = name.t.as_str();
            let pkg = purple_garden_std::resolve_pkg(pkg_name)?;
            let (_, variants) = pkg
                .overload_groups()
                .into_iter()
                .find(|(name, _)| *name == fn_name)?;
            Some(AnalysisHover {
                span: token_span(name),
                markup: HoverMarkup::Markdown(overload_detail(
                    &format!("{pkg_name}.{fn_name}"),
                    &variants,
                )),
            })
        }
        _ => None,
    }
}

fn local_function_detail(ast: &Ast<'_>, query: &str) -> Option<String> {
    ast.roots.iter().find_map(|&root| match ast.node(root) {
        Node::Fn {
            name,
            args,
            return_type,
            ..
        } if name.t.as_str() == query => Some(fn_detail(ast, name, args, *return_type)),
        _ => None,
    })
}

fn overload_detail(name: &str, variants: &[&purple_garden_runtime::Fn<'_>]) -> String {
    let display_name = name.rsplit_once('.').map_or(name, |(_, name)| name);
    let out = crate::doc::render_function(display_name, variants);
    format!("{}\n{}", out.trim_end(), crate::doc::command(name))
}

fn import_hover(pkg: &Token<'_>) -> Option<String> {
    package_hover(pkg.t.as_str())
}

fn package_target_hover(ast: &Ast<'_>, target: NodeId) -> Option<(Span, String)> {
    let Node::Ident { name, .. } = ast.node(target) else {
        return None;
    };
    package_hover(name.t.as_str()).map(|detail| (token_span(name), detail))
}

fn package_hover(pkg_name: &str) -> Option<String> {
    let pkg = purple_garden_std::resolve_pkg(pkg_name)?;
    let header = format!("import \"{pkg_name}\"");
    let command = crate::doc::command(pkg_name);
    if pkg.doc.is_empty() {
        Some(format!("{header}\n\n{command}"))
    } else {
        Some(format!("{header}\n\n{}\n\n{command}", pkg.doc))
    }
}

fn garden_block(contents: impl std::fmt::Display) -> String {
    format!("```garden\n{}\n```", contents)
}

fn type_for_node(
    ast: &Ast<'_>,
    typecheck: &TypecheckOutput<'_>,
    node_id: NodeId,
) -> Option<String> {
    ast.value_id(node_id)
        .and_then(|id| typecheck.types.get(id))
        .and_then(Option::as_ref)
        .map(ToString::to_string)
}
