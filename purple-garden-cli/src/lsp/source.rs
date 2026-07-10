use lsp_types::{Position, Range, TextDocumentContentChangeEvent};
use purple_garden_frontend::{
    ast::{Ast, Node, NodeId, TypeExprId},
    diagnostic::Span,
    lex::Token,
};

pub(super) fn node_span(ast: &Ast<'_>, node_id: NodeId) -> Option<Span> {
    let node = ast.node(node_id);
    match node {
        Node::Atom { raw, .. } | Node::Ident { name: raw, .. } => Some(token_span(raw)),
        Node::Bin { op, lhs, rhs, .. } => Some(cover_spans(&[
            node_span(ast, *lhs)?,
            token_span(op),
            node_span(ast, *rhs)?,
        ])),
        Node::Unary { op, rhs, .. } => Some(cover_spans(&[token_span(op), node_span(ast, *rhs)?])),
        Node::Array { src, members, .. } => {
            let mut spans = vec![token_span(src)];
            for &member in members {
                spans.push(node_span(ast, member)?);
            }
            Some(cover_spans(&spans))
        }
        Node::Record { src, fields, .. } => {
            let mut spans = vec![token_span(src)];
            for (field, value) in fields {
                spans.push(token_span(field));
                spans.push(node_span(ast, *value)?);
            }
            Some(cover_spans(&spans))
        }
        Node::Let { name, rhs, .. } => {
            Some(cover_spans(&[token_span(name), node_span(ast, *rhs)?]))
        }
        Node::Fn { name, body, .. } => Some(if let Some(body_span) = cover_node_list(ast, body) {
            cover_spans(&[token_span(name), body_span])
        } else {
            token_span(name)
        }),
        Node::Match { cases, default, .. } => {
            let mut spans = Vec::new();
            for &((ref token, condition), ref body) in cases {
                spans.push(token_span(token));
                spans.push(node_span(ast, condition)?);
                if let Some(body_span) = cover_node_list(ast, body) {
                    spans.push(body_span);
                }
            }
            spans.push(token_span(&default.0));
            if let Some(default_span) = cover_node_list(ast, &default.1) {
                spans.push(default_span);
            }
            Some(cover_spans(&spans))
        }
        Node::Call { target, args, .. } => {
            let mut spans = vec![node_span(ast, *target)?];
            for &arg in args {
                spans.push(node_span(ast, arg)?);
            }
            Some(cover_spans(&spans))
        }
        Node::Field { target, name, .. } => {
            Some(cover_spans(&[node_span(ast, *target)?, token_span(name)]))
        }
        Node::Cast { src, lhs, rhs, .. } => Some(cover_spans(&[
            node_span(ast, *lhs)?,
            token_span(src),
            type_expr_span(ast, *rhs),
        ])),
        Node::Import { src, pkgs, .. } => {
            let mut spans = vec![token_span(src)];
            spans.extend(pkgs.iter().map(token_span));
            Some(cover_spans(&spans))
        }
        Node::Extern { src, name, fns, .. } => {
            let mut spans = vec![token_span(src), token_span(name)];
            for fun in fns {
                spans.push(token_span(&fun.name));
                for (arg, ty) in &fun.args {
                    spans.push(token_span(arg));
                    spans.push(type_expr_span(ast, *ty));
                }
                spans.push(type_expr_span(ast, fun.return_type));
            }
            Some(cover_spans(&spans))
        }
    }
}

pub(super) fn type_expr_span(ast: &Ast<'_>, id: TypeExprId) -> Span {
    token_span(ast.type_token(id))
}

pub(super) fn token_span(token: &Token<'_>) -> Span {
    let len = token.t.as_str().len();
    if matches!(token.t, purple_garden_frontend::lex::Type::S(_)) {
        Span::new(token.start.saturating_add(1), len)
    } else {
        Span::new(token.start, len)
    }
}

pub(super) fn span_contains(span: Span, offset: usize) -> bool {
    let end = span.start.saturating_add(span.len.max(1));
    span.start <= offset && offset < end
}

pub(super) fn range_for_span(source: &str, span: Span) -> Range {
    let start = position_for_offset(source, span.start);
    let end_offset = span.start.saturating_add(span.len.max(1));
    let end = position_for_offset(source, end_offset);
    Range { start, end }
}

pub(super) fn range_for_edit(source: &str, span: Span) -> Range {
    let start = position_for_offset(source, span.start);
    let end = position_for_offset(source, span.start.saturating_add(span.len));
    Range { start, end }
}

pub(super) fn offset_for_position(source: &str, position: Position) -> usize {
    let mut line_start = 0;
    for _ in 0..position.line {
        let Some(next_newline) = source[line_start..].find('\n') else {
            return source.len();
        };
        line_start += next_newline + 1;
    }

    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |idx| line_start + idx);
    let line = &source[line_start..line_end];
    let mut utf16_units = 0;
    for (idx, ch) in line.char_indices() {
        if utf16_units >= position.character {
            return line_start + idx;
        }
        utf16_units += ch.len_utf16() as u32;
    }
    line_end
}

pub(super) fn apply_content_changes(
    source: &mut String,
    changes: Vec<TextDocumentContentChangeEvent>,
) {
    for change in changes {
        if let Some(range) = change.range {
            let start = offset_for_position(source, range.start);
            let end = offset_for_position(source, range.end);
            source.replace_range(start..end, &change.text);
        } else {
            *source = change.text;
        }
    }
}

fn cover_node_list(ast: &Ast<'_>, nodes: &[NodeId]) -> Option<Span> {
    let spans = nodes
        .iter()
        .copied()
        .map(|node| node_span(ast, node))
        .collect::<Option<Vec<_>>>()?;
    (!spans.is_empty()).then(|| cover_spans(&spans))
}

fn cover_spans(spans: &[Span]) -> Span {
    let start = spans
        .iter()
        .map(|span| span.start)
        .min()
        .unwrap_or_default();
    let end = spans
        .iter()
        .map(|span| span.start.saturating_add(span.len))
        .max()
        .unwrap_or(start);
    Span::new(start, end.saturating_sub(start))
}

fn position_for_offset(source: &str, offset: usize) -> Position {
    let clamped = offset.min(source.len());
    let prefix = &source[..clamped];
    let line = prefix.bytes().filter(|&b| b == b'\n').count() as u32;
    let line_start = prefix
        .as_bytes()
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |idx| idx + 1);
    let character = prefix[line_start..].encode_utf16().count() as u32;
    Position { line, character }
}
