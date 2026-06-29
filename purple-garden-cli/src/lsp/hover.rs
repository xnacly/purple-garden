use purple_garden_frontend::{
    ast::{Ast, Node, NodeId, TypeExprId},
    diagnostic::Span,
    lex::{Lexer, Token, Type},
    typecheck::TypecheckOutput,
};

use super::{analysis::DocumentAnalysis, source::token_span};

pub(super) struct AnalysisHover {
    pub(super) span: Span,
    pub(super) markup: HoverMarkup,
}

pub(super) enum HoverMarkup {
    Garden(String),
    Markdown(String),
}

pub(super) fn collect_lexical_hovers(source: &str, analysis: &mut DocumentAnalysis) {
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
        Type::Extern => (LanguageDocKind::Keyword, "extern"),
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
        Type::Option => (LanguageDocKind::Type, "Option"),
        Type::Array => (LanguageDocKind::Type, "Array"),
        Type::Foreign => (LanguageDocKind::Type, "Foreign"),
        Type::Record => (LanguageDocKind::Type, "Record"),
        _ => return None,
    })
}

pub(super) fn fn_detail(
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

pub(super) fn add_decl_hover(
    analysis: &mut DocumentAnalysis,
    span: Span,
    detail: String,
    docs: &[Token<'_>],
) {
    if docs.is_empty() {
        analysis.add_garden_hover(span, detail);
    } else {
        let contents = doc_hover(&detail, docs);
        analysis.add_resolved_markdown_hover(span, contents.clone());
        analysis.add_declaration_doc(span, contents);
    }
}

pub(super) fn doc_hover(detail: &str, docs: &[Token<'_>]) -> String {
    format!("{}\n\n{}", garden_block(detail), doc_text(docs))
}

pub(super) fn declaration_hover(detail: &str, docs: &[Token<'_>], query: &str) -> String {
    let body = if docs.is_empty() {
        garden_block(detail)
    } else {
        doc_hover(detail, docs)
    };
    format!("{}\n{}", body.trim_end(), crate::doc::command(query))
}

fn doc_text(docs: &[Token<'_>]) -> String {
    docs.iter()
        .map(|doc| doc.t.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn call_hover(
    ast: &Ast<'_>,
    target: NodeId,
    analysis: &DocumentAnalysis,
) -> Option<AnalysisHover> {
    match ast.node(target) {
        Node::Ident { name, .. } => {
            local_function_hover(ast, name.t.as_str()).map(|markup| AnalysisHover {
                span: token_span(name),
                markup,
            })
        }
        Node::Field { target, name, .. } => {
            let Node::Ident { name: pkg, .. } = ast.node(*target) else {
                return None;
            };
            let pkg_name = pkg.t.as_str();
            let fn_name = name.t.as_str();
            function_hover(pkg_name, fn_name, analysis).map(|contents| AnalysisHover {
                span: token_span(name),
                markup: HoverMarkup::Markdown(contents),
            })
        }
        _ => None,
    }
}

pub(super) fn ident_hover(
    ast: &Ast<'_>,
    typecheck: &TypecheckOutput<'_>,
    node_id: NodeId,
    name: &Token<'_>,
) -> Option<HoverMarkup> {
    local_function_hover(ast, name.t.as_str()).or_else(|| {
        type_for_node(ast, typecheck, node_id)
            .map(|ty| HoverMarkup::Garden(format!("{}: {}", name.t.as_str(), ty)))
    })
}

fn local_function_hover(ast: &Ast<'_>, query: &str) -> Option<HoverMarkup> {
    ast.roots.iter().find_map(|&root| match ast.node(root) {
        Node::Fn {
            docs,
            name,
            args,
            return_type,
            ..
        } if name.t.as_str() == query => {
            let detail = fn_detail(ast, name, args, *return_type);
            Some(if docs.is_empty() {
                HoverMarkup::Garden(detail)
            } else {
                HoverMarkup::Markdown(doc_hover(&detail, docs))
            })
        }
        _ => None,
    })
}

fn overload_detail(name: &str, variants: &[&purple_garden_runtime::Fn<'_>]) -> String {
    let display_name = name.rsplit_once('.').map_or(name, |(_, name)| name);
    let out = crate::doc::render_function(display_name, variants);
    format!("{}\n{}", out.trim_end(), crate::doc::command(name))
}

fn function_hover(pkg_name: &str, fn_name: &str, analysis: &DocumentAnalysis) -> Option<String> {
    if let Some(contents) = analysis
        .package_docs
        .get(pkg_name)
        .and_then(|pkg| pkg.functions.get(fn_name))
    {
        return Some(contents.clone());
    }

    let pkg = purple_garden_std::resolve_pkg(pkg_name)?;
    let (_, variants) = pkg
        .overload_groups()
        .into_iter()
        .find(|(name, _)| *name == fn_name)?;
    Some(overload_detail(&format!("{pkg_name}.{fn_name}"), &variants))
}

pub(super) fn import_hover(pkg: &Token<'_>, analysis: &DocumentAnalysis) -> Option<String> {
    package_hover(pkg.t.as_str(), analysis)
}

pub(super) fn package_target_hover(
    ast: &Ast<'_>,
    target: NodeId,
    analysis: &DocumentAnalysis,
) -> Option<(Span, String)> {
    let Node::Ident { name, .. } = ast.node(target) else {
        return None;
    };
    package_hover(name.t.as_str(), analysis).map(|detail| (token_span(name), detail))
}

fn package_hover(pkg_name: &str, analysis: &DocumentAnalysis) -> Option<String> {
    if let Some(doc) = analysis.package_docs.get(pkg_name) {
        return Some(format!(
            "{}\n{}",
            doc.hover.trim_end(),
            crate::doc::command(pkg_name)
        ));
    }

    let pkg = purple_garden_std::resolve_pkg(pkg_name)?;
    let header = format!("import \"{pkg_name}\"");
    let command = crate::doc::command(pkg_name);
    if pkg.doc.is_empty() {
        Some(format!("{header}\n\n{command}"))
    } else {
        Some(format!("{header}\n\n{}\n\n{command}", pkg.doc))
    }
}

pub(super) fn garden_block(contents: impl std::fmt::Display) -> String {
    format!("```garden\n{}\n```", contents)
}

pub(super) fn type_for_node(
    ast: &Ast<'_>,
    typecheck: &TypecheckOutput<'_>,
    node_id: NodeId,
) -> Option<String> {
    ast.value_id(node_id)
        .and_then(|id| typecheck.types.get(id))
        .and_then(Option::as_ref)
        .map(ToString::to_string)
}
