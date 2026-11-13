//! The building blocks every view is made of: square, flat and monochrome, in the spirit of
//! sourcehut. Code is the only colorful thing, the chrome uses the foreground, a muted gray, the
//! border and one accent marking what's selected.

use gpui_kit::{
    component::{ActiveTheme as _, Theme},
    prelude::FluentBuilder as _,
    *,
};

/// Square corners, no shadows, the chrome in a small sans serif.
pub fn flatten_theme(cx: &mut App) {
    let theme = Theme::global_mut(cx);
    theme.radius = px(0.);
    theme.radius_lg = px(0.);
    theme.shadow = false;
    theme.font_size = px(13.);
}

/// The accent marking selection, a link blue like sourcehut's.
pub fn accent(cx: &App) -> Hsla {
    cx.theme().blue
}

/// A tab of a tab bar: the selected one is underlined in the accent.
pub fn tab(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    div()
        .id(id)
        .h_full()
        .px_3()
        .flex()
        .items_center()
        .gap_1()
        .border_b_2()
        .cursor_pointer()
        .border_color(if selected {
            accent(cx)
        } else {
            transparent_black()
        })
        .text_color(if selected {
            theme.foreground
        } else {
            theme.muted_foreground
        })
        .when(selected, |d| d.font_weight(FontWeight::SEMIBOLD))
        .hover(|d| d.text_color(theme.foreground))
        .child(label.into())
}

/// A selectable entry of a list: the selected one is filled and edged in the accent.
pub fn item(id: impl Into<ElementId>, selected: bool, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .border_l_2()
        .cursor_pointer()
        .border_color(if selected {
            accent(cx)
        } else {
            transparent_black()
        })
        .when(selected, |d| d.bg(theme.secondary))
        .hover(|d| d.bg(theme.secondary_hover))
}

/// A rectangular button with a 1px border.
pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    div()
        .id(id)
        .px_2()
        .py(px(1.))
        .border_1()
        .border_color(theme.border)
        .cursor_pointer()
        .text_xs()
        .hover(|d| d.bg(theme.secondary_hover))
        .child(label.into())
}

/// A bar of a pane, under its tabs or above its content.
pub fn bar(cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_2()
        .px_3()
        .h(px(28.))
        .border_b_1()
        .border_color(theme.border)
        .text_xs()
        .text_color(theme.muted_foreground)
}

/// A small uppercase heading over a section.
pub fn heading(label: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .pt_3()
        .pb_1()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().muted_foreground)
        .child(label.into().to_uppercase())
}

/// A small square marking how a function runs: filled for native code, hollow for bytecode,
/// faint when it doesn't run at all.
pub fn marker(native: bool, unused: bool, cx: &App) -> Div {
    let theme = cx.theme();
    let square = div().flex_none().size(px(7.)).border_1();
    if unused {
        square.border_color(theme.border)
    } else if native {
        square.border_color(accent(cx)).bg(accent(cx))
    } else {
        square.border_color(theme.muted_foreground)
    }
}
