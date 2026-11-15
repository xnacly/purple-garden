use lsp_types::{Position, Range, TextDocumentContentChangeEvent};
use purple_garden_frontend::{
    ast::{Ast, Node, NodeId, TypeExprId},
    diagnostic::Span,
    lex::Token,
};

pub(super) fn node_span(ast: &Ast<'_, '_>, node_id: NodeId) -> Option<Span> {
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
            for &member in *members {
                spans.push(node_span(ast, member)?);
            }
            Some(cover_spans(&spans))
        }
        Node::Record { src, fields, .. } => {
            let mut spans = vec![token_span(src)];
            for (field, value) in *fields {
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
            for &((ref token, condition), body) in *cases {
                spans.push(token_span(token));
                spans.push(node_span(ast, condition)?);
                if let Some(body_span) = cover_node_list(ast, body) {
                    spans.push(body_span);
                }
            }
            spans.push(token_span(&default.0));
            if let Some(default_span) = cover_node_list(ast, default.1) {
                spans.push(default_span);
            }
            Some(cover_spans(&spans))
        }
        Node::Call { target, args, .. } => {
            let mut spans = vec![node_span(ast, *target)?];
            for &arg in *args {
                spans.push(node_span(ast, arg)?);
            }
            Some(cover_spans(&spans))
        }
        Node::Field { target, name, .. } => {
            Some(cover_spans(&[node_span(ast, *target)?, token_span(name)]))
        }
        Node::Index { target, index, .. } => Some(cover_spans(&[
            node_span(ast, *target)?,
            node_span(ast, *index)?,
        ])),
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
            for fun in *fns {
                spans.push(token_span(&fun.name));
                for (arg, ty) in fun.args {
                    spans.push(token_span(arg));
                    spans.push(type_expr_span(ast, *ty));
                }
                spans.push(type_expr_span(ast, fun.return_type));
            }
            Some(cover_spans(&spans))
        }
    }
}

pub(super) fn type_expr_span(ast: &Ast<'_, '_>, id: TypeExprId) -> Span {
    token_span(ast.type_token(id))
}

pub(super) fn token_span(token: &Token<'_>) -> Span {
    Span::from_token(token)
}

pub(super) fn span_contains(span: Span, offset: usize) -> bool {
    let end = span.start.saturating_add(span.len.max(1));
    span.start <= offset && offset < end
}

/// An open document's text with the byte offset of every line start, so converting between byte
/// offsets and LSP positions costs a binary search and a walk of one line. Both conversions are
/// total: offsets past the end or inside a character, and positions past a line or the document
/// clamp to the nearest character boundary before them.
#[derive(Debug, Default)]
pub(super) struct Source {
    text: String,
    line_starts: Vec<usize>,
}

impl Source {
    pub(super) fn new(text: String) -> Self {
        let line_starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        Self { text, line_starts }
    }

    pub(super) fn text(&self) -> &str {
        &self.text
    }

    pub(super) fn into_text(self) -> String {
        self.text
    }

    /// Apply the edits of a `didChange` in order, each relative to the text the previous left.
    #[must_use]
    pub(super) fn edit(mut self, changes: Vec<TextDocumentContentChangeEvent>) -> Self {
        for change in changes {
            let Some(range) = change.range else {
                self = Self::new(change.text);
                continue;
            };
            let start = self.offset(range.start);
            // a reversed range covers nothing
            let end = self.offset(range.end).max(start);
            let mut text = self.text;
            text.replace_range(start..end, &change.text);
            self = Self::new(text);
        }
        self
    }

    /// The range `span` underlines, at least one character.
    pub(super) fn range(&self, span: Span) -> Range {
        Range {
            start: self.position(span.start),
            end: self.position(span.start.saturating_add(span.len.max(1))),
        }
    }

    /// The range `span` replaces, possibly empty.
    pub(super) fn edit_range(&self, span: Span) -> Range {
        Range {
            start: self.position(span.start),
            end: self.position(span.start.saturating_add(span.len)),
        }
    }

    pub(super) fn position(&self, offset: usize) -> Position {
        let offset = self.text.floor_char_boundary(offset);
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let character = self.text[self.line_starts[line]..offset].encode_utf16().count();
        Position {
            line: u32::try_from(line).unwrap_or(u32::MAX),
            character: u32::try_from(character).unwrap_or(u32::MAX),
        }
    }

    pub(super) fn offset(&self, position: Position) -> usize {
        let Some(&line_start) = self.line_starts.get(position.line as usize) else {
            return self.text.len();
        };
        let line_end = self
            .line_starts
            .get(position.line as usize + 1)
            .map_or(self.text.len(), |next| next - 1);
        let line = &self.text[line_start..line_end];
        let line = line.strip_suffix('\r').unwrap_or(line);
        let mut utf16_units = 0;
        for (idx, ch) in line.char_indices() {
            if utf16_units >= position.character {
                return line_start + idx;
            }
            utf16_units += ch.len_utf16() as u32;
        }
        line_start + line.len()
    }
}

fn cover_node_list(ast: &Ast<'_, '_>, nodes: &[NodeId]) -> Option<Span> {
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

#[cfg(test)]
mod tests {
    use lsp_types::{Position, Range, TextDocumentContentChangeEvent};
    use purple_garden_frontend::diagnostic::Span;

    use super::Source;

    const TEXT: &str = "let é = \"😀\"\r\nx😀y\n\nend";

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    /// Every byte offset maps to a position, inside a character to the one it starts at, and that
    /// position maps back to the character's first byte. A line ends before its `\r\n`.
    #[test]
    fn every_offset_has_a_position() {
        let source = Source::new(TEXT.to_owned());
        for offset in 0..=TEXT.len() + 8 {
            let boundary = TEXT.floor_char_boundary(offset);
            let position = source.position(offset);
            assert_eq!(source.position(boundary), position, "offset {offset}");
            // between `\r` and `\n` is no position, the line ends before both
            let back = if TEXT[..boundary].ends_with('\r') && TEXT[boundary..].starts_with('\n') {
                boundary - 1
            } else {
                boundary
            };
            assert_eq!(source.offset(position), back, "offset {offset}");
        }
        assert_eq!(source.position(5), pos(0, 4), "inside `é`");
        assert_eq!(source.position(TEXT.find('x').unwrap()), pos(1, 0));
        assert_eq!(source.position(TEXT.len()), pos(3, 3));
    }

    #[test]
    fn positions_past_their_line_or_the_document_clamp() {
        let source = Source::new(TEXT.to_owned());
        let line_1 = TEXT.find('x').unwrap();
        // the second half of the surrogate pair of 😀 starts no character, the next one does
        assert_eq!(source.offset(pos(1, 2)), line_1 + 1 + 4);
        assert_eq!(source.offset(pos(1, 3)), line_1 + 1 + 4);
        assert_eq!(source.offset(pos(1, u32::MAX)), line_1 + 6, "before the newline");
        assert_eq!(source.offset(pos(0, u32::MAX)), TEXT.find('\r').unwrap(), "before \\r\\n");
        assert_eq!(source.offset(pos(2, 7)), TEXT.rfind('\n').unwrap());
        assert_eq!(source.offset(pos(u32::MAX, 0)), TEXT.len());
        let past = Span::new(usize::MAX - 1, 5);
        assert_eq!(source.range(past), Range::new(pos(3, 3), pos(3, 3)));
    }

    #[test]
    fn edits_apply_in_order_and_tolerate_any_range() {
        let change = |range: Option<(Position, Position)>, text: &str| {
            TextDocumentContentChangeEvent {
                range: range.map(|(start, end)| Range::new(start, end)),
                range_length: None,
                text: text.to_owned(),
            }
        };
        let source = Source::new("ab😀cd\nef".to_owned()).edit(vec![
            change(Some((pos(0, 1), pos(0, 2))), "B"),
            // reversed: inserts at its start
            change(Some((pos(0, 5), pos(0, 1))), "<"),
        ]);
        assert_eq!(source.text(), "aB😀c<d\nef");
        let source = source.edit(vec![
            // from inside the surrogate pair past the line, then past the document
            change(Some((pos(0, 3), pos(0, 99))), "|"),
            change(Some((pos(99, 0), pos(99, 9))), "!"),
        ]);
        assert_eq!(source.text(), "aB😀|\nef!");
        let source = source.edit(vec![change(None, "new\n")]);
        assert_eq!(source.text(), "new\n");
        assert_eq!(source.position(4), pos(1, 0));
    }
}
