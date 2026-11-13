//! Register allocation as a timeline: a lane per register or slot, a bar per value spanning its
//! live interval in the lane it was allocated to.

use gpui_kit::{component::ActiveTheme as _, *};
use purple_garden_inspect::{
    Function, Location,
    view::registers::{lanes, pressure},
};

use super::{mono, placeholder};
use crate::{
    app::{Target, Workspace},
    ui,
};

const LANE: Pixels = px(22.);
const ROW: Pixels = px(16.);
const LABELS: Pixels = px(64.);

fn locations(f: &Function, target: Target) -> Result<&[Option<Location>], String> {
    match target {
        Target::Bytecode if f.bc_locations.is_empty() => {
            Err("Not lowered to bytecode, so no registers were allocated".to_string())
        }
        Target::Bytecode => Ok(&f.bc_locations),
        Target::X86 => f
            .x86
            .as_ref()
            .map(|n| n.locations.as_slice())
            .map_err(|reason| format!("Not compiled to x86-64: {reason}")),
    }
}

pub fn render(ws: &Workspace, cx: &mut Context<Workspace>) -> AnyElement {
    let functions = ws.shown();
    if functions.is_empty() {
        return placeholder("No function to show", cx).into_any_element();
    }
    let target = ws.target;
    let fg = cx.theme().foreground;
    let toggle = div()
        .flex()
        .h_full()
        .children([Target::Bytecode, Target::X86].map(|t| {
            ui::item(t.label(), t == target, cx)
                .h_full()
                .text_color(fg)
                .child(t.label())
                .on_click(cx.listener(move |ws, _, _, cx| ws.select_target(t, cx)))
        }));
    let header = ui::bar(cx)
        .justify_between()
        .pr_0()
        .child("a lane per register and stack slot, a bar from a value's definition to its last use, live values on top")
        .child(toggle);

    let muted = cx.theme().muted_foreground;
    let mut body = div()
        .id("registers")
        .size_full()
        .overflow_scroll()
        .font(mono(cx))
        .p_2();
    for f in functions {
        body = body.child(
            div()
                .pt_2()
                .pb_1()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(format!("f{} <{}>", f.id, f.name)),
        );
        body = match locations(f, target) {
            Err(reason) => body.child(div().pb_2().text_xs().text_color(muted).child(reason)),
            Ok(locations) => body.child(timeline(ws, f, locations, cx)),
        };
    }
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(header)
        .child(div().flex_1().min_h_0().child(body))
        .into_any_element()
}

fn timeline(
    ws: &Workspace,
    f: &Function,
    locations: &[Option<Location>],
    cx: &mut Context<Workspace>,
) -> impl IntoElement {
    let theme = cx.theme();
    let lanes = lanes(&f.intervals, locations);
    let pressure = pressure(&f.intervals);
    let rows = pressure.len() as f32;
    let max_pressure = pressure.iter().copied().max().unwrap_or(1).max(1) as f32;
    let palette = [
        theme.blue,
        theme.green,
        theme.magenta,
        theme.cyan,
        theme.yellow,
        theme.red,
    ];
    let hovered_row = ws
        .links()
        .hovered
        .filter(|(func, _)| *func == f.id)
        .map(|(_, row)| row);
    let blocks: Vec<(u32, u32)> = f
        .final_ir()
        .rows
        .iter()
        .filter(|r| r.indent == 1)
        .filter_map(|r| Some((r.pos?, r.block?)))
        .collect();

    let width = LABELS + ROW * rows + px(24.);
    let chart_h = px(36.);
    let height = chart_h + px(18.) + LANE * lanes.len() as f32 + px(12.);
    let mut canvas = div().relative().w(width).h(height);

    // pressure histogram
    for (row, &live) in pressure.iter().enumerate() {
        let h = chart_h * (live as f32 / max_pressure);
        canvas = canvas.child(
            div()
                .absolute()
                .left(LABELS + ROW * row as f32 + px(1.))
                .top(chart_h - h)
                .w(ROW - px(2.))
                .h(h)
                .bg(theme.blue.opacity(0.35)),
        );
    }
    // block boundaries
    let top = chart_h + px(18.);
    for &(pos, block) in &blocks {
        canvas = canvas
            .child(
                div()
                    .absolute()
                    .left(LABELS + ROW * pos as f32)
                    .top(chart_h + px(2.))
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("b{block}")),
            )
            .child(
                div()
                    .absolute()
                    .left(LABELS + ROW * pos as f32)
                    .top(top)
                    .w(px(1.))
                    .h(LANE * lanes.len() as f32)
                    .bg(theme.border),
            );
    }
    if let Some(row) = hovered_row {
        canvas = canvas.child(
            div()
                .absolute()
                .left(LABELS + ROW * row as f32)
                .top(px(0.))
                .w(ROW)
                .h(height)
                .bg(theme.list_hover.opacity(0.6)),
        );
    }
    let workspace = cx.entity().downgrade();
    for (lane, (location, bars)) in lanes.iter().enumerate() {
        let y = top + LANE * lane as f32;
        canvas = canvas.child(
            div()
                .absolute()
                .left(px(8.))
                .top(y + px(3.))
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(location.to_string()),
        );
        for bar in bars {
            let (def, last) = bar.rows;
            let color = palette[bar.value as usize % palette.len()];
            let hovered = hovered_row.is_some_and(|r| r >= def && r <= last);
            let workspace = workspace.clone();
            let def_row = (f.id, def);
            // from the middle of the defining row to the middle of the last use: a register freed
            // by a value's last use is taken by the value defined on that very row
            let (left, width) = if def == last {
                (ROW * def as f32 + ROW / 4., ROW / 2.)
            } else {
                (ROW * def as f32 + ROW / 2., ROW * (last - def) as f32)
            };
            canvas = canvas.child(
                div()
                    .id(SharedString::from(format!("bar-{}-{}", f.id, bar.value)))
                    .absolute()
                    .left(LABELS + left + px(1.))
                    .top(y + px(2.))
                    .w(width - px(2.))
                    .h(LANE - px(4.))
                    .bg(color.opacity(if hovered { 0.55 } else { 0.28 }))
                    .border_1()
                    .border_color(color.opacity(0.8))
                    .overflow_hidden()
                    .px_1()
                    .text_xs()
                    .text_color(theme.foreground)
                    .child(format!("%v{}", bar.value))
                    .on_hover(move |h, _, cx| {
                        if let Some(workspace) = workspace.upgrade() {
                            workspace.update(cx, |ws, cx| ws.hover_row(Some(def_row), *h, cx));
                        }
                    }),
            );
        }
    }
    canvas
}
