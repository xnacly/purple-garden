use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
};

use lsp_types::{
    CompletionItem, CompletionItemKind, GotoDefinitionResponse, Hover, HoverContents, Location,
    MarkupContent, MarkupKind, Position, Uri,
};
use purple_garden_frontend::{
    diagnostic::{Diagnostic as FrontendDiagnostic, Span},
    typemap::TypeRef,
};
use purple_garden_ir::ptype::Type;

use super::{
    completion::{self, CompletionEntry, RecordCompletion},
    diagnostic::diagnostic_for_lsp,
    hover::{AnalysisHover, HoverMarkup},
    source::{Source, span_contains},
    text::Text,
};

#[derive(Default)]
pub(super) struct DocumentState {
    source: Source,
    analysis: DocumentAnalysis,
}

#[derive(Debug, Clone, Default)]
pub(super) struct DocumentAnalysis {
    pub(super) diagnostics: Vec<FrontendDiagnostic>,
    pub(super) hovers: Vec<HoverEntry>,
    /// Every distinct hover text, which [`DocumentAnalysis::hovers`] share, see [`Text`].
    hover_texts: HashSet<Arc<str>>,
    /// The type of every type map entry shown so far, see [`super::hover::type_text`].
    pub(super) type_texts: HashMap<TypeRef, Arc<str>>,
    /// Field completions of every record type map entry seen so far.
    pub(super) record_types: HashMap<TypeRef, Option<Rc<RecordCompletion>>>,
    pub(super) definitions: Vec<DefinitionEntry>,
    /// Docs of the declaration at a span, shown on hovering its references.
    declaration_docs: HashMap<Span, Text>,
    /// Hover of every function the document declares, by name.
    pub(super) local_functions: HashMap<String, HoverMarkup>,
    pub(super) package_docs: HashMap<String, PackageDoc>,
    pub(super) imported_packages: Vec<String>,
    pub(super) completions: Vec<CompletionEntry>,
    pub(super) record_completions: HashMap<String, Rc<RecordCompletion>>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct PackageDoc {
    pub(super) hover: String,
    pub(super) functions: HashMap<String, String>,
    pub(super) completions: HashMap<String, PackageFunctionCompletion>,
}

#[derive(Debug, Clone)]
pub(super) struct PackageFunctionCompletion {
    pub(super) detail: String,
    pub(super) documentation: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct HoverEntry {
    pub(super) span: Span,
    pub(super) contents: Text,
    pub(super) priority: HoverPriority,
}

#[derive(Debug, Clone)]
pub(super) struct DefinitionEntry {
    pub(super) reference: Span,
    pub(super) definition: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum HoverPriority {
    Resolved,
    Lexical,
    Type,
}

impl DocumentState {
    pub(super) fn analyze(path: Option<PathBuf>, text: String) -> Self {
        let mut analysis = DocumentAnalysis::new();
        super::hover::collect_lexical_hovers(&text, &mut analysis);

        if let Some(path) = path.as_deref() {
            crate::frontend::analyze_path(
                path,
                text.as_bytes(),
                Vec::new(),
                purple_garden_std::STD,
                |frontend| {
                    super::collect::collect_frontend_analysis(frontend, &mut analysis);
                },
            );
        } else {
            crate::frontend::analyze(
                text.as_bytes(),
                Vec::new(),
                purple_garden_std::STD,
                |frontend| {
                    super::collect::collect_frontend_analysis(frontend, &mut analysis);
                },
            );
        }

        Self {
            source: Source::new(text),
            analysis,
        }
    }

    pub(super) fn diagnostics(&self) -> Vec<lsp_types::Diagnostic> {
        self.analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.primary.span.start < self.source.text().len())
            .map(|diagnostic| diagnostic_for_lsp(diagnostic, &self.source))
            .collect()
    }

    pub(super) fn into_source(self) -> Source {
        self.source
    }

    pub(super) fn code_actions(
        &self,
        uri: lsp_types::Uri,
        range: lsp_types::Range,
    ) -> lsp_types::CodeActionResponse {
        super::code_action::actions_for(uri, &self.source, &self.analysis.diagnostics, range)
    }

    pub(super) fn hover_at(&self, position: Position) -> Option<Hover> {
        let offset = self.source.offset(position);
        let entry = self
            .analysis
            .hovers
            .iter()
            .filter(|entry| entry.span.start < self.source.text().len())
            .filter(|entry| span_contains(entry.span, offset))
            .min_by_key(|entry| (entry.span.len, entry.priority))?;

        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: entry.contents.to_string(),
            }),
            range: Some(self.source.range(entry.span)),
        })
    }

    pub(super) fn definition_at(
        &self,
        uri: Uri,
        position: Position,
    ) -> Option<GotoDefinitionResponse> {
        let offset = self.source.offset(position);
        let entry = self
            .analysis
            .definitions
            .iter()
            .filter(|entry| entry.reference.start < self.source.text().len())
            .filter(|entry| span_contains(entry.reference, offset))
            .min_by_key(|entry| entry.reference.len)?;

        Some(GotoDefinitionResponse::Scalar(Location::new(
            uri,
            self.source.range(entry.definition),
        )))
    }

    pub(super) fn completions_at(&self, position: Position) -> Vec<CompletionItem> {
        let offset = self.source.offset(position);
        completion::items_at(
            &self.analysis.completions,
            &self.analysis.record_completions,
            &self.analysis.package_docs,
            &self.analysis.imported_packages,
            self.source.text(),
            offset,
        )
    }
}

impl DocumentAnalysis {
    fn new() -> Self {
        Self {
            completions: completion::global_completions().to_vec(),
            ..Default::default()
        }
    }

    pub(super) fn add_garden_hover(&mut self, span: Span, contents: impl Into<Text>) {
        self.add_garden_hover_with_priority(span, contents, HoverPriority::Type);
    }

    pub(super) fn add_garden_hover_with_priority(
        &mut self,
        span: Span,
        contents: impl Into<Text>,
        priority: HoverPriority,
    ) {
        self.push_hover(span, contents.into().garden(), priority);
    }

    pub(super) fn add_markdown_hover(&mut self, span: Span, contents: impl Into<Text>) {
        self.add_markdown_hover_with_priority(span, contents, HoverPriority::Lexical);
    }

    pub(super) fn add_resolved_markdown_hover(&mut self, span: Span, contents: impl Into<Text>) {
        self.add_markdown_hover_with_priority(span, contents, HoverPriority::Resolved);
    }

    pub(super) fn add_markdown_hover_with_priority(
        &mut self,
        span: Span,
        contents: impl Into<Text>,
        priority: HoverPriority,
    ) {
        self.push_hover(span, contents.into(), priority);
    }

    fn push_hover(&mut self, span: Span, contents: Text, priority: HoverPriority) {
        let contents = contents.interned(&mut self.hover_texts);
        self.hovers.push(HoverEntry {
            span,
            contents,
            priority,
        });
    }

    pub(super) fn add_hover(&mut self, hover: AnalysisHover) {
        match hover.markup {
            HoverMarkup::Garden(contents) => self.add_garden_hover(hover.span, contents),
            HoverMarkup::Markdown(contents) => self.add_markdown_hover(hover.span, contents),
        }
    }

    pub(super) fn add_completion(
        &mut self,
        label: impl Into<String>,
        kind: CompletionItemKind,
        detail: Option<Text>,
    ) {
        self.completions
            .push(CompletionEntry::local(label, kind, detail));
    }

    pub(super) fn add_record_completion(&mut self, name: impl Into<String>, ty: &Type<'_>) {
        if let Some(record) = RecordCompletion::from_type(ty) {
            self.record_completions.insert(name.into(), Rc::new(record));
        }
    }

    /// [`DocumentAnalysis::add_record_completion`] for the type map entry `ty`, whose completions
    /// every binding of it shares.
    pub(super) fn add_shared_record_completion(
        &mut self,
        name: impl Into<String>,
        ty: TypeRef,
        resolved: &Type<'_>,
    ) {
        let record = self
            .record_types
            .entry(ty)
            .or_insert_with(|| RecordCompletion::from_type(resolved).map(Rc::new));
        if let Some(record) = record {
            self.record_completions.insert(name.into(), Rc::clone(record));
        }
    }

    pub(super) fn add_imported_package(&mut self, pkg: &str) {
        if !self
            .imported_packages
            .iter()
            .any(|imported| imported == pkg)
        {
            self.imported_packages.push(pkg.to_owned());
        }
    }

    pub(super) fn add_declaration_doc(&mut self, definition: Span, contents: Text) {
        self.declaration_docs.entry(definition).or_insert(contents);
    }

    pub(super) fn add_reference_doc_hovers(&mut self) {
        let mut hovers = Vec::new();
        for entry in &self.definitions {
            if entry.reference == entry.definition {
                continue;
            }
            if let Some(contents) = self.declaration_docs.get(&entry.definition) {
                hovers.push((entry.reference, contents.clone()));
            }
        }
        for (span, contents) in hovers {
            self.add_resolved_markdown_hover(span, contents);
        }
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::Position;
    use purple_garden_frontend::limits::MAX_DIAGNOSTICS;

    use super::DocumentState;

    /// A character the lexer doesn't know spans all its bytes, so its diagnostic converts to a
    /// range, and so does every request around it.
    #[test]
    fn stray_non_ascii_characters_are_diagnosed() {
        let state = DocumentState::analyze(None, "let t = é\n😀 x".to_owned());
        let ranges: Vec<_> = state
            .diagnostics()
            .iter()
            .map(|d| (d.range.start.line, d.range.start.character, d.range.end.character))
            .collect();
        assert!(ranges.contains(&(0, 8, 9)), "{ranges:?}");
        assert!(ranges.contains(&(1, 0, 2)), "{ranges:?}");
        for character in 0..12 {
            let position = Position::new(1, character);
            let _ = state.hover_at(position);
            let _ = state.completions_at(position);
        }
    }

    /// Every read of a binding shows its type, which is formatted once and shared, so a wide type
    /// read many times costs memory once.
    #[test]
    fn reads_share_the_text_of_their_type() {
        let fields = (0..200)
            .map(|i| format!("field_{i}: {i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let reads = "r\n".repeat(1000);
        let state = DocumentState::analyze(None, format!("let r = {{ {fields} }}\n{reads}"));
        let typed: Vec<_> = state
            .analysis
            .hovers
            .iter()
            .filter(|hover| hover.contents.to_string().contains("Record<field_0: Int"))
            .collect();
        assert!(typed.len() >= 1000, "{} typed hovers", typed.len());
        assert!(typed.iter().all(|hover| hover.contents.shares(&typed[0].contents)));
        let hover = state.hover_at(Position::new(500, 0)).expect("a read has a hover");
        let lsp_types::HoverContents::Markup(markup) = hover.contents else {
            panic!("hovers are markup");
        };
        assert!(markup.value.starts_with("```garden\nRecord<field_0: Int"));
    }

    #[test]
    fn diagnostics_stop_at_the_limit() {
        let state = DocumentState::analyze(None, "missing ".repeat(10_000));
        assert_eq!(state.diagnostics().len(), MAX_DIAGNOSTICS);
        let state = DocumentState::analyze(None, "é".repeat(10_000));
        assert_eq!(state.diagnostics().len(), MAX_DIAGNOSTICS);
    }
}
