//! The IR of the shown functions after the selected pass, diffed against the IR before it.

use std::rc::Rc;

use gpui_kit::{component::ActiveTheme as _, *};
use purple_garden_inspect::{
    Function,
    view::listing::{Line, diffed},
};

use super::{listing, mono, placeholder};
use crate::{app::Workspace, ui};

/// The passes of `functions`, those that changed one stand out, clicking one selects it.
pub fn passes(
    functions: &[&Function],
    selected: usize,
    cx: &mut Context<Workspace>,
) -> impl IntoElement {
    let snapshots = &functions[0].snapshots;
    let theme = cx.theme();
    let (fg, muted) = (theme.foreground, theme.muted_foreground);
    div()
        .id("passes")
        .flex_none()
        .w(px(190.))
        .h_full()
        .overflow_y_scroll()
        .border_r_1()
        .border_color(theme.border)
        .py_1()
        .font(mono(cx))
        .text_xs()
        .children(snapshots.iter().enumerate().map(|(ix, snapshot)| {
            let changed = ix == 0
                || functions
                    .iter()
                    .any(|f| f.snapshots.get(ix).is_some_and(|s| s.ir.is_some()));
            ui::item(ix, ix == selected, cx)
                .py(px(3.))
                .justify_between()
                .text_color(if changed { fg } else { muted })
                .child(snapshot.pass.clone())
                .child(
                    div()
                        .text_color(muted)
                        .child(if changed && ix > 0 { "+" } else { "" }),
                )
                .on_click(cx.listener(move |ws, _, _, cx| ws.select_pass(ix, cx)))
        }))
}

pub fn render(ws: &Workspace, cx: &mut Context<Workspace>) -> AnyElement {
    let functions = ws.shown();
    let Some(first) = functions.first() else {
        return placeholder("No function to show", cx).into_any_element();
    };
    let pass = ws.pass(first);
    let mut lines = Vec::new();
    for f in &functions {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.extend(diffed(f, pass.min(f.snapshots.len() - 1)));
    }
    div()
        .size_full()
        .flex()
        .child(passes(&functions, pass, cx))
        .child(div().flex_1().h_full().child(listing(
            "ir",
            Rc::new(lines),
            ws.links(),
            &ws.scroll.ir,
            cx,
        )))
        .into_any_element()
}
