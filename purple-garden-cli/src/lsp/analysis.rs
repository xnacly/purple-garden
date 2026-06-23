use std::{collections::HashMap, path::PathBuf};

use lsp_types::{
    CompletionItem, CompletionItemKind, GotoDefinitionResponse, Hover, HoverContents, Location,
    MarkupContent, MarkupKind, Position, Uri,
};
use purple_garden_frontend::diagnostic::{Diagnostic as FrontendDiagnostic, Span};
use purple_garden_ir::ptype::Type;

use super::{
    completion::{self, CompletionEntry, RecordCompletion},
    diagnostic::diagnostic_for_lsp,
    hover::{AnalysisHover, HoverMarkup},
    source::{offset_for_position, range_for_span, span_contains},
};

#[derive(Default)]
pub(super) struct DocumentState {
    text: String,
    analysis: DocumentAnalysis,
}

#[derive(Debug, Clone, Default)]
pub(super) struct DocumentAnalysis {
    pub(super) diagnostics: Vec<FrontendDiagnostic>,
    pub(super) hovers: Vec<HoverEntry>,
    pub(super) definitions: Vec<DefinitionEntry>,
    declaration_docs: Vec<DeclarationDoc>,
    pub(super) package_docs: HashMap<String, PackageDoc>,
    pub(super) imported_packages: Vec<String>,
    pub(super) completions: Vec<CompletionEntry>,
    pub(super) record_completions: HashMap<String, RecordCompletion>,
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
    pub(super) contents: String,
    pub(super) priority: HoverPriority,
}

#[derive(Debug, Clone)]
pub(super) struct DefinitionEntry {
    pub(super) reference: Span,
    pub(super) definition: Span,
}

#[derive(Debug, Clone)]
struct DeclarationDoc {
    definition: Span,
    contents: String,
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
            crate::frontend::analyze_path(path, text.as_bytes(), Vec::new(), |frontend| {
                super::collect::collect_frontend_analysis(frontend, &mut analysis);
            });
        } else {
            crate::frontend::analyze(text.as_bytes(), Vec::new(), |frontend| {
                super::collect::collect_frontend_analysis(frontend, &mut analysis);
            });
        }

        Self { text, analysis }
    }

    pub(super) fn diagnostics(&self) -> Vec<lsp_types::Diagnostic> {
        self.analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.primary.span.start < self.text.len())
            .map(|diagnostic| diagnostic_for_lsp(diagnostic, &self.text))
            .collect()
    }

    pub(super) fn text(&self) -> &str {
        &self.text
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
            .filter(|entry| entry.span.start < self.text.len())
            .filter(|entry| span_contains(entry.span, offset))
            .min_by_key(|entry| (entry.span.len, entry.priority))?;

        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: entry.contents.clone(),
            }),
            range: Some(range_for_span(&self.text, entry.span)),
        })
    }

    pub(super) fn definition_at(
        &self,
        uri: Uri,
        position: Position,
    ) -> Option<GotoDefinitionResponse> {
        let offset = offset_for_position(&self.text, position);
        let entry = self
            .analysis
            .definitions
            .iter()
            .filter(|entry| entry.reference.start < self.text.len())
            .filter(|entry| span_contains(entry.reference, offset))
            .min_by_key(|entry| entry.reference.len)?;

        Some(GotoDefinitionResponse::Scalar(Location::new(
            uri,
            range_for_span(&self.text, entry.definition),
        )))
    }

    pub(super) fn completions_at(&self, position: Position) -> Vec<CompletionItem> {
        let offset = offset_for_position(&self.text, position);
        completion::items_at(
            &self.analysis.completions,
            &self.analysis.record_completions,
            &self.analysis.package_docs,
            &self.analysis.imported_packages,
            &self.text,
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

    pub(super) fn add_garden_hover(&mut self, span: Span, contents: impl Into<String>) {
        self.add_garden_hover_with_priority(span, contents, HoverPriority::Type);
    }

    pub(super) fn add_garden_hover_with_priority(
        &mut self,
        span: Span,
        contents: impl Into<String>,
        priority: HoverPriority,
    ) {
        self.hovers.push(HoverEntry {
            span,
            contents: super::hover::garden_block(contents.into()),
            priority,
        });
    }

    pub(super) fn add_markdown_hover(&mut self, span: Span, contents: impl Into<String>) {
        self.add_markdown_hover_with_priority(span, contents, HoverPriority::Lexical);
    }

    pub(super) fn add_resolved_markdown_hover(&mut self, span: Span, contents: impl Into<String>) {
        self.add_markdown_hover_with_priority(span, contents, HoverPriority::Resolved);
    }

    pub(super) fn add_markdown_hover_with_priority(
        &mut self,
        span: Span,
        contents: impl Into<String>,
        priority: HoverPriority,
    ) {
        self.hovers.push(HoverEntry {
            span,
            contents: contents.into(),
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
        detail: Option<String>,
    ) {
        self.completions
            .push(CompletionEntry::local(label, kind, detail));
    }

    pub(super) fn add_record_completion(&mut self, name: impl Into<String>, ty: &Type<'_>) {
        if let Some(record) = RecordCompletion::from_type(ty) {
            self.record_completions.insert(name.into(), record);
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

    pub(super) fn add_declaration_doc(&mut self, definition: Span, contents: String) {
        self.declaration_docs.push(DeclarationDoc {
            definition,
            contents,
        });
    }

    pub(super) fn add_reference_doc_hovers(&mut self) {
        let mut hovers = Vec::new();
        for entry in &self.definitions {
            if entry.reference == entry.definition {
                continue;
            }
            if let Some(doc) = self
                .declaration_docs
                .iter()
                .find(|doc| doc.definition == entry.definition)
            {
                hovers.push((entry.reference, doc.contents.clone()));
            }
        }
        for (span, contents) in hovers {
            self.add_resolved_markdown_hover(span, contents);
        }
    }
}
