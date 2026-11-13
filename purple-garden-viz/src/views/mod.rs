//! The panes next to the editor. Every listing is a [`Line`] list rendered by [`listing`], so the
//! IR, bytecode, x86-64 and dumps share hover linking, highlighting and virtualized scrolling.

pub mod cfg;
pub mod code;
pub mod ir;
pub mod memory;
pub mod output;
pub mod registers;

use std::{collections::HashSet, rc::Rc};

use gpui_kit::{component::ActiveTheme as _, prelude::FluentBuilder as _, *};
pub use purple_garden_inspect::view::listing::{Line, RowRef, Tone};

use crate::{app::Workspace, highlight::styled};

/// Which lines are linked to the editor's cursor and the hovered row.
#[derive(Debug, Clone, Default)]
pub struct Links {
    /// Rows lowered from the source line the cursor is on.
    pub active: Rc<HashSet<RowRef>>,
    pub hovered: Option<RowRef>,
}

/// Monospace text in the theme's code font.
pub fn mono(cx: &App) -> Font {
    font(cx.theme().mono_font_family.clone())
}

pub const LINE_HEIGHT: Pixels = px(18.);
pub const TEXT_SIZE: Pixels = px(12.5);

/// A virtualized listing of `lines`, hovering a line hovers its row in every view.
pub fn listing(
    id: impl Into<ElementId>,
    lines: Rc<Vec<Line>>,
    links: Links,
    scroll: &UniformListScrollHandle,
    cx: &mut Context<Workspace>,
) -> impl IntoElement {
    let workspace = cx.entity().downgrade();
    let gutter_width = lines.iter().map(|l| l.gutter.len()).max().unwrap_or(0);
    uniform_list(id, lines.len(), move |range, _, cx| {
        let theme = cx.theme();
        let (hover_bg, active_bg) = (theme.list_hover, theme.list_active.opacity(0.5));
        let (added, removed) = (theme.green.opacity(0.14), theme.red.opacity(0.14));
        let muted = theme.muted_foreground;
        range
            .map(|ix| {
                let line = &lines[ix];
                let bg = match (line.row, line.tone) {
                    (Some(row), _) if links.hovered == Some(row) => Some(hover_bg),
                    (_, Tone::Added) => Some(added),
                    (_, Tone::Removed) => Some(removed),
                    (Some(row), _) if links.active.contains(&row) => Some(active_bg),
                    _ => None,
                };
                let text = StyledText::new(line.text.clone())
                    .with_highlights(styled(&line.highlights, cx));
                let workspace = workspace.clone();
                let row = line.row;
                div()
                    .id(ix)
                    .w_full()
                    .h(LINE_HEIGHT)
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_3()
                    .whitespace_nowrap()
                    .when_some(bg, |d, bg| d.bg(bg))
                    .when(line.tone == Tone::Label, |d| d.mt(px(0.)))
                    .when(line.tone == Tone::Removed, |d| d.line_through())
                    .when(matches!(line.tone, Tone::Muted), |d| d.text_color(muted))
                    .child(
                        div()
                            .w(px(gutter_width as f32 * 7.6))
                            .flex_none()
                            .text_color(muted)
                            .child(line.gutter.clone()),
                    )
                    .child(text)
                    .on_hover(move |hovered, _, cx| {
                        if let Some(workspace) = workspace.upgrade() {
                            workspace.update(cx, |ws, cx| ws.hover_row(row, *hovered, cx));
                        }
                    })
            })
            .collect()
    })
    .track_scroll(scroll)
    .font(mono(cx))
    .text_size(TEXT_SIZE)
    .size_full()
}

/// A centered note in an empty pane.
pub fn placeholder(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .text_color(cx.theme().muted_foreground)
        .text_sm()
        .child(text.into())
}
