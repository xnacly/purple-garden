use lsp_types::{DiagnosticSeverity, NumberOrString};
use purple_garden_frontend::diagnostic::Diagnostic as FrontendDiagnostic;

use super::source::Source;

pub(super) fn diagnostic_for_lsp(
    diagnostic: &FrontendDiagnostic,
    source: &Source,
) -> lsp_types::Diagnostic {
    lsp_types::Diagnostic {
        range: source.range(diagnostic.primary.span),
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String("purple-garden".to_owned())),
        code_description: None,
        source: Some("purple-garden".to_owned()),
        message: diagnostic_message(diagnostic),
        related_information: None,
        tags: None,
        data: None,
    }
}

fn diagnostic_message(diagnostic: &FrontendDiagnostic) -> String {
    let mut message = diagnostic.message.clone();
    if let Some(label) = &diagnostic.primary.message {
        message.push('\n');
        message.push_str(label);
    }
    for note in &diagnostic.notes {
        message.push_str("\nnote: ");
        message.push_str(note);
    }
    for help in &diagnostic.helps {
        message.push_str("\nhelp: ");
        message.push_str(&help.message);
    }
    message
}
