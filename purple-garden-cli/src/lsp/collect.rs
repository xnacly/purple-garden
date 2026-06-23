use std::collections::HashMap;

use lsp_types::CompletionItemKind;
use purple_garden_frontend::{
    ast::{Ast, Node, NodeId},
    typecheck::TypecheckOutput,
};

use super::{
    analysis::{DocumentAnalysis, PackageDoc},
    definition::DefinitionCollector,
    hover::{
        AnalysisHover, add_decl_hover, call_hover, declaration_hover, doc_hover, fn_detail,
        garden_block, ident_hover, import_hover, package_target_hover, type_for_node,
    },
    source::{node_span, token_span, type_expr_span},
};

pub(super) fn collect_frontend_analysis(
    frontend: crate::frontend::FrontendAnalysis<'_, '_>,
    analysis: &mut DocumentAnalysis,
) {
    if let (Some(ast), Some(typecheck)) = (frontend.ast, frontend.typecheck) {
        collect_package_docs(ast, analysis);
        for &root in &ast.roots {
            collect_node(ast, typecheck, root, analysis);
        }
        analysis.definitions = DefinitionCollector::collect(ast);
        analysis.add_reference_doc_hovers();
    }
    analysis.diagnostics = frontend.diagnostics.to_vec();
}

fn collect_package_docs(ast: &Ast<'_>, analysis: &mut DocumentAnalysis) {
    for &root in &ast.roots {
        let Node::Extern {
            docs, name, fns, ..
        } = ast.node(root)
        else {
            continue;
        };
        let pkg_name = name.t.as_str();
        let detail = format!("extern {}", name.t.as_str());
        let hover = if docs.is_empty() {
            garden_block(detail)
        } else {
            doc_hover(&detail, docs)
        };

        let mut functions = HashMap::new();
        for fun in fns {
            let detail = fn_detail(ast, &fun.name, &fun.args, fun.return_type);
            let query = format!("{}.{}", pkg_name, fun.name.t.as_str());
            functions.insert(
                fun.name.t.as_str().to_owned(),
                declaration_hover(&detail, &fun.docs, &query),
            );
        }

        analysis
            .package_docs
            .insert(pkg_name.to_owned(), PackageDoc { hover, functions });
    }
}

fn collect_node(
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
            docs,
            name,
            args,
            return_type,
            body,
        } => {
            let detail = fn_detail(ast, name, args, *return_type);
            add_decl_hover(analysis, token_span(name), detail.clone(), docs);
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
        Node::Let {
            docs, name, rhs, ..
        } => {
            if let Some(ty) = type_for_node(ast, typecheck, *rhs) {
                let detail = format!("{}: {}", name.t.as_str(), ty);
                add_decl_hover(analysis, token_span(name), detail.clone(), docs);
                analysis.add_completion(
                    name.t.as_str(),
                    CompletionItemKind::VARIABLE,
                    Some(detail),
                );
            }
            collect_node(ast, typecheck, *rhs, analysis);
        }
        Node::Field { target, name, .. } => {
            if let Some((span, detail)) = package_target_hover(ast, *target, analysis) {
                analysis.add_resolved_markdown_hover(span, detail);
            }
            if let Some(ty) = type_for_node(ast, typecheck, node_id) {
                analysis
                    .add_garden_hover(token_span(name), format!(".{}: {}", name.t.as_str(), ty));
            }
            collect_node(ast, typecheck, *target, analysis);
        }
        Node::Cast { lhs, rhs, .. } => {
            collect_node(ast, typecheck, *lhs, analysis);
            analysis.add_garden_hover(
                type_expr_span(ast, *rhs),
                ast.type_display(*rhs).to_string(),
            );
        }
        Node::Bin { lhs, rhs, .. } => collect_nodes(ast, typecheck, &[*lhs, *rhs], analysis),
        Node::Unary { rhs, .. } => collect_node(ast, typecheck, *rhs, analysis),
        Node::Array { members, .. } => collect_nodes(ast, typecheck, members, analysis),
        Node::Object { pairs, .. } => {
            for &(key, value) in pairs {
                collect_node(ast, typecheck, key, analysis);
                collect_node(ast, typecheck, value, analysis);
            }
        }
        Node::Record { fields, .. } => {
            for (field, value) in fields {
                if let Some(ty) = type_for_node(ast, typecheck, *value) {
                    analysis.add_garden_hover(
                        token_span(field),
                        format!("{}: {}", field.t.as_str(), ty),
                    );
                }
                collect_node(ast, typecheck, *value, analysis);
            }
        }
        Node::Match { cases, default, .. } => {
            for &((_, condition), ref body) in cases {
                collect_node(ast, typecheck, condition, analysis);
                collect_nodes(ast, typecheck, body, analysis);
            }
            collect_nodes(ast, typecheck, &default.1, analysis);
        }
        Node::Call { target, args, .. } => {
            if let Some(hover) = call_hover(ast, *target, analysis) {
                analysis.add_hover(hover);
            }
            collect_node(ast, typecheck, *target, analysis);
            collect_nodes(ast, typecheck, args, analysis);
        }
        Node::Import { pkgs, .. } => {
            for pkg in pkgs {
                analysis.add_imported_package(pkg.t.as_str());
                if let Some(detail) = import_hover(pkg, analysis) {
                    analysis.add_resolved_markdown_hover(token_span(pkg), detail);
                }
            }
        }
        Node::Extern {
            docs, name, fns, ..
        } => {
            let detail = format!("extern {}", name.t.as_str());
            add_decl_hover(analysis, token_span(name), detail, docs);
            for fun in fns {
                let detail = fn_detail(ast, &fun.name, &fun.args, fun.return_type);
                add_decl_hover(analysis, token_span(&fun.name), detail.clone(), &fun.docs);
                analysis.add_completion(
                    fun.name.t.as_str(),
                    CompletionItemKind::FUNCTION,
                    Some(detail),
                );
                for (arg, ty) in &fun.args {
                    analysis.add_garden_hover(
                        token_span(arg),
                        format!("{}: {}", arg.t.as_str(), ast.type_display(*ty)),
                    );
                }
            }
        }
        Node::Ident { name, .. } => {
            if let Some(hover) = ident_hover(ast, typecheck, node_id, name) {
                analysis.add_hover(AnalysisHover {
                    span: token_span(name),
                    markup: hover,
                });
            }
        }
        Node::Atom { .. } => {}
    }
}

fn collect_nodes(
    ast: &Ast<'_>,
    typecheck: &TypecheckOutput<'_>,
    nodes: &[NodeId],
    analysis: &mut DocumentAnalysis,
) {
    for &node in nodes {
        collect_node(ast, typecheck, node, analysis);
    }
}
