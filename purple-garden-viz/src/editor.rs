//! The source editor: highlighted by the compiler's lexer, hovers, completions and definitions
//! answered by the same analysis the language server runs.

use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    component::input::{
        CompletionProvider, DefinitionProvider, EditorState, HoverProvider, InputHighlighter, Rope,
    },
    *,
};
use lsp_types::{CompletionContext, CompletionResponse, Hover, LocationLink, Position, Uri};
use purple_garden_analysis::{DocumentState, position_for_offset};

use crate::highlight::{GardenHighlighter, LANGUAGE};

/// The analysis of the editor's text, redone whenever a provider asks about a text it didn't see.
#[derive(Clone, Default)]
pub struct Analysis(Rc<RefCell<Option<DocumentState>>>);

impl Analysis {
    fn with<R>(&self, text: &Rope, f: impl FnOnce(&DocumentState, &str) -> R) -> R {
        let text = text.to_string();
        let mut state = self.0.borrow_mut();
        if state.as_ref().is_none_or(|s| s.text() != text) {
            *state = Some(DocumentState::analyze(None, text.clone()));
        }
        f(state.as_ref().expect("analysed above"), &text)
    }
}

fn uri() -> Uri {
    "file:///editor.garden".parse().expect("a valid uri")
}

impl HoverProvider for Analysis {
    fn hover(
        &self,
        text: &Rope,
        offset: usize,
        _: &mut Window,
        _: &mut App,
    ) -> Task<anyhow::Result<Option<Hover>>> {
        let hover = self.with(text, |state, text| {
            state.hover_at(position_for_offset(text, offset))
        });
        Task::ready(Ok(hover))
    }
}

impl DefinitionProvider for Analysis {
    fn definitions(
        &self,
        text: &Rope,
        offset: usize,
        _: &mut Window,
        _: &mut App,
    ) -> Task<anyhow::Result<Vec<LocationLink>>> {
        let links = self.with(text, |state, text| {
            let Some(lsp_types::GotoDefinitionResponse::Scalar(location)) =
                state.definition_at(uri(), position_for_offset(text, offset))
            else {
                return Vec::new();
            };
            vec![LocationLink {
                origin_selection_range: None,
                target_uri: location.uri,
                target_range: location.range,
                target_selection_range: location.range,
            }]
        });
        Task::ready(Ok(links))
    }
}

impl CompletionProvider for Analysis {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        _: CompletionContext,
        _: &mut Window,
        _: &mut App,
    ) -> Task<anyhow::Result<CompletionResponse>> {
        let items = self.with(text, |state, text| {
            state.completions_at(position_for_offset(text, offset))
        });
        Task::ready(Ok(CompletionResponse::Array(items)))
    }

    fn is_completion_trigger(&self, _: usize, new_text: &str, _: &mut App) -> bool {
        new_text
            .chars()
            .last()
            .is_some_and(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '"'))
    }
}

/// A purple garden editor holding `source`.
pub fn new(source: &str, window: &mut Window, cx: &mut Context<EditorState>) -> EditorState {
    let mut state = EditorState::new(window, cx)
        .language(LANGUAGE)
        .soft_wrap(false);
    state.set_highlighter_factory(
        Rc::new(|language: &str| {
            (language == LANGUAGE)
                .then(|| Box::new(GardenHighlighter::default()) as Box<dyn InputHighlighter>)
        }),
        cx,
    );
    let analysis = Analysis::default();
    let lsp = state.lsp_mut();
    lsp.hover_provider = Some(Rc::new(analysis.clone()));
    lsp.definition_provider = Some(Rc::new(analysis.clone()));
    lsp.completion_provider = Some(Rc::new(analysis));
    state.set_value(source.to_string(), window, cx);
    state
}

/// The line and column of `offset` in `text`, the editor's diagnostics and cursor speak lines.
#[must_use]
pub fn position(text: &str, offset: usize) -> Position {
    position_for_offset(text, offset.min(text.len()))
}
