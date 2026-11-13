use std::{collections::HashMap, path::PathBuf};

use lsp_types::{
    CompletionItem, CompletionItemKind, GotoDefinitionResponse, Hover, HoverContents, Location,
    MarkupContent, MarkupKind, Position, Uri,
};
use purple_garden_frontend::diagnostic::{Diagnostic as FrontendDiagnostic, Span};
use purple_garden_ir::ptype::Type;

use crate::{
    completion::{self, CompletionEntry, RecordCompletion},
    diagnostic::diagnostic_for_lsp,
    hover::{AnalysisHover, HoverMarkup},
    source::{offset_for_position, range_for_span, span_contains},
};

#[derive(Default)]
pub struct DocumentState {
    text: String,
    analysis: DocumentAnalysis,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DocumentAnalysis {
    pub(crate) diagnostics: Vec<FrontendDiagnostic>,
    pub(crate) hovers: Vec<HoverEntry>,
    pub(crate) definitions: Vec<DefinitionEntry>,
    declaration_docs: Vec<DeclarationDoc>,
    pub(crate) package_docs: HashMap<String, PackageDoc>,
    pub(crate) imported_packages: Vec<String>,
    pub(crate) completions: Vec<CompletionEntry>,
    pub(crate) record_completions: HashMap<String, RecordCompletion>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PackageDoc {
    pub(crate) hover: String,
    pub(crate) functions: HashMap<String, String>,
    pub(crate) completions: HashMap<String, PackageFunctionCompletion>,
}

#[derive(Debug, Clone)]
pub(crate) struct PackageFunctionCompletion {
    pub(crate) detail: String,
    pub(crate) documentation: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct HoverEntry {
    pub(crate) span: Span,
    pub(crate) contents: String,
    pub(crate) priority: HoverPriority,
}

#[derive(Debug, Clone)]
pub(crate) struct DefinitionEntry {
    pub(crate) reference: Span,
    pub(crate) definition: Span,
}

#[derive(Debug, Clone)]
struct DeclarationDoc {
    definition: Span,
    contents: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum HoverPriority {
    Resolved,
    Lexical,
    Type,
}

impl DocumentState {
    pub fn analyze(path: Option<PathBuf>, text: String) -> Self {
        let mut analysis = DocumentAnalysis::new();
        crate::hover::collect_lexical_hovers(&text, &mut analysis);

        if let Some(path) = path.as_deref() {
            crate::frontend::analyze_path(
                path,
                text.as_bytes(),
                Vec::new(),
                purple_garden_std::STD,
                |frontend| {
                    crate::collect::collect_frontend_analysis(frontend, &mut analysis);
                },
            );
        } else {
            crate::frontend::analyze(
                text.as_bytes(),
                Vec::new(),
                purple_garden_std::STD,
                |frontend| {
                    crate::collect::collect_frontend_analysis(frontend, &mut analysis);
                },
            );
        }

        Self { text, analysis }
    }

    pub fn diagnostics(&self) -> Vec<lsp_types::Diagnostic> {
        self.analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.primary.span.start < self.text.len())
            .map(|diagnostic| diagnostic_for_lsp(diagnostic, &self.text))
            .collect()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn code_actions(
        &self,
        uri: lsp_types::Uri,
        range: lsp_types::Range,
    ) -> lsp_types::CodeActionResponse {
        crate::code_action::actions_for(uri, &self.text, &self.analysis.diagnostics, range)
    }

    pub fn hover_at(&self, position: Position) -> Option<Hover> {
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

    pub fn definition_at(&self, uri: Uri, position: Position) -> Option<GotoDefinitionResponse> {
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

    pub fn completions_at(&self, position: Position) -> Vec<CompletionItem> {
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

    pub(crate) fn add_garden_hover(&mut self, span: Span, contents: impl Into<String>) {
        self.add_garden_hover_with_priority(span, contents, HoverPriority::Type);
    }

    pub(crate) fn add_garden_hover_with_priority(
        &mut self,
        span: Span,
        contents: impl Into<String>,
        priority: HoverPriority,
    ) {
        self.hovers.push(HoverEntry {
            span,
            contents: crate::hover::garden_block(contents.into()),
            priority,
        });
    }

    pub(crate) fn add_markdown_hover(&mut self, span: Span, contents: impl Into<String>) {
        self.add_markdown_hover_with_priority(span, contents, HoverPriority::Lexical);
    }

    pub(crate) fn add_resolved_markdown_hover(&mut self, span: Span, contents: impl Into<String>) {
        self.add_markdown_hover_with_priority(span, contents, HoverPriority::Resolved);
    }

    pub(crate) fn add_markdown_hover_with_priority(
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

    pub(crate) fn add_hover(&mut self, hover: AnalysisHover) {
        match hover.markup {
            HoverMarkup::Garden(contents) => self.add_garden_hover(hover.span, contents),
            HoverMarkup::Markdown(contents) => self.add_markdown_hover(hover.span, contents),
        }
    }

    pub(crate) fn add_completion(
        &mut self,
        label: impl Into<String>,
        kind: CompletionItemKind,
        detail: Option<String>,
    ) {
        self.completions
            .push(CompletionEntry::local(label, kind, detail));
    }

    pub(crate) fn add_record_completion(&mut self, name: impl Into<String>, ty: &Type<'_>) {
        if let Some(record) = RecordCompletion::from_type(ty) {
            self.record_completions.insert(name.into(), record);
        }
    }

    pub(crate) fn add_imported_package(&mut self, pkg: &str) {
        if !self
            .imported_packages
            .iter()
            .any(|imported| imported == pkg)
        {
            self.imported_packages.push(pkg.to_owned());
        }
    }

    pub(crate) fn add_declaration_doc(&mut self, definition: Span, contents: String) {
        self.declaration_docs.push(DeclarationDoc {
            definition,
            contents,
        });
    }

    pub(crate) fn add_reference_doc_hovers(&mut self) {
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
