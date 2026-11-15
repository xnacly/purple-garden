use std::collections::HashMap;

use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionResponse, Range, TextEdit, Uri,
    WorkspaceEdit,
};
use purple_garden_frontend::diagnostic::Diagnostic as FrontendDiagnostic;

use super::{
    diagnostic::diagnostic_for_lsp,
    source::Source,
};

pub(super) fn actions_for(
    uri: Uri,
    source: &Source,
    diagnostics: &[FrontendDiagnostic],
    request_range: Range,
) -> CodeActionResponse {
    diagnostics
        .iter()
        .filter(|diagnostic| {
            intersects(
                source.range(diagnostic.primary.span),
                request_range,
            )
        })
        .flat_map(|diagnostic| actions_for_diagnostic(uri.clone(), source, diagnostic))
        .collect()
}

fn actions_for_diagnostic(
    uri: Uri,
    source: &Source,
    diagnostic: &FrontendDiagnostic,
) -> Vec<CodeActionOrCommand> {
    diagnostic
        .helps
        .iter()
        .filter_map(|help| {
            let replacement = help.replacement.as_ref()?;
            Some(CodeActionOrCommand::CodeAction(CodeAction {
                title: help.message.clone(),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![diagnostic_for_lsp(diagnostic, source)]),
                edit: Some(workspace_edit(
                    uri.clone(),
                    TextEdit::new(
                        source.edit_range(replacement.span),
                        replacement.text.clone(),
                    ),
                )),
                command: None,
                is_preferred: Some(true),
                disabled: None,
                data: None,
            }))
        })
        .collect()
}

fn workspace_edit(uri: Uri, edit: TextEdit) -> WorkspaceEdit {
    WorkspaceEdit {
        changes: Some(HashMap::from([(uri, vec![edit])])),
        document_changes: None,
        change_annotations: None,
    }
}

fn intersects(lhs: Range, rhs: Range) -> bool {
    position_le(lhs.start, rhs.end) && position_le(rhs.start, lhs.end)
}

fn position_le(lhs: lsp_types::Position, rhs: lsp_types::Position) -> bool {
    lhs.line < rhs.line || (lhs.line == rhs.line && lhs.character <= rhs.character)
}
