//! Bytecode, x86-64 and the plain text dumps.

use std::rc::Rc;

use gpui_kit::*;
use purple_garden_inspect::view::listing::{bytecode_lines, text_lines, x86_lines};

use super::{listing, placeholder};
use crate::{app::Workspace, ui};

pub fn render_bytecode(ws: &Workspace, cx: &mut Context<Workspace>) -> AnyElement {
    let functions = ws.shown();
    let lines = bytecode_lines(&ws.inspection, (!ws.all).then_some(&functions[..]));
    if lines.is_empty() {
        let why = match functions.first() {
            Some(f) if f.unused => "Unused, so never lowered to bytecode",
            Some(_) => "No bytecode",
            None => "No function to show",
        };
        return placeholder(why, cx).into_any_element();
    }
    listing(
        "bytecode",
        Rc::new(lines),
        ws.links(),
        &ws.scroll.bytecode,
        cx,
    )
    .into_any_element()
}

pub fn render_x86(ws: &Workspace, cx: &mut Context<Workspace>) -> AnyElement {
    let functions = ws.shown();
    if functions.is_empty() {
        return placeholder("No function to show", cx).into_any_element();
    }
    let natives: Vec<_> = functions
        .iter()
        .filter_map(|f| f.x86.as_ref().ok())
        .collect();
    let size: usize = natives.iter().map(|n| n.size).sum();
    let note = format!(
        "{} of {} functions, {size} bytes, compiled by the x86-64 backend{}",
        natives.len(),
        functions.len(),
        if cfg!(target_arch = "x86_64") {
            ""
        } else {
            " · shown only, this host can't run it"
        }
    );
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(ui::bar(cx).child(note))
        .child(div().flex_1().child(listing(
            "x86",
            Rc::new(x86_lines(&functions)),
            ws.links(),
            &ws.scroll.x86,
            cx,
        )))
        .into_any_element()
}

pub fn render_text(
    id: &'static str,
    text: &str,
    empty: &str,
    cx: &mut Context<Workspace>,
    scroll: &UniformListScrollHandle,
) -> AnyElement {
    if text.trim().is_empty() {
        return placeholder(empty.to_string(), cx).into_any_element();
    }
    listing(
        id,
        Rc::new(text_lines(text)),
        super::Links::default(),
        scroll,
        cx,
    )
    .into_any_element()
}
