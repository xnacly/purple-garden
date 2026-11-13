//! The control flow graphs of the shown functions after the selected pass, see
//! [`purple_garden_inspect::view::cfg`] for the layout.

use std::{collections::HashMap, rc::Rc};

use gpui_kit::{component::ActiveTheme as _, prelude::FluentBuilder as _, *};
use purple_garden_inspect::{
    Ir,
    view::cfg::{LINE, Layout, PAD, program},
};

use super::{mono, placeholder};
use crate::{app::Workspace, highlight::styled, ui};

const FONT: f32 = 11.;
/// Where the graph starts, below the zoom toolbar.
const TOP: Pixels = px(52.);

/// What a layout was computed for: the function (`None` for all), pass and compile.
pub type LayoutKey = (Option<u32>, usize, u64);

/// Pan and zoom of the graph.
#[derive(Debug, Clone)]
pub struct View {
    pub zoom: f32,
    pub pan: Point<Pixels>,
    /// Where the pointer was when dragging started, and the pan then.
    pub drag: Option<(Point<Pixels>, Point<Pixels>)>,
    /// The layout and what it was computed for: the function (`None` for all), pass and
    /// compile.
    pub cache: Option<(LayoutKey, Rc<Layout>)>,
    /// The size of the pane, recorded while painting.
    pub viewport: Rc<std::cell::Cell<Size<Pixels>>>,
    /// Fit the graph once the pane's size is known.
    pub pending_fit: bool,
}

impl View {
    /// Zoom and pan so `layout` fits the pane, never zooming in past 100%.
    pub fn fit(&mut self, layout: &Layout) {
        let viewport = self.viewport.get();
        let (w, h) = layout.size;
        let margin = 48.;
        if viewport.width <= px(margin) || w <= 0. || h <= 0. {
            return;
        }
        let zoom = (f32::from(viewport.width) - margin) / w;
        let zoom = zoom
            .min((f32::from(viewport.height - TOP) - margin) / h)
            .clamp(0.2, 1.);
        self.zoom = zoom;
        self.pan = point(((viewport.width - px(w * zoom)) / 2.).max(px(24.)), TOP);
    }
}

impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 1.,
            pan: point(px(24.), TOP),
            drag: None,
            cache: None,
            viewport: Rc::default(),
            pending_fit: false,
        }
    }
}

fn char_width(window: &Window, cx: &App) -> f32 {
    let text = window.text_system();
    let font_id = text.resolve_font(&mono(cx));
    text.advance(font_id, px(FONT), 'm')
        .map_or(FONT * 0.6, |s| f32::from(s.width))
}

pub fn render(ws: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) -> AnyElement {
    let functions = ws.shown();
    let Some(first) = functions.first() else {
        return placeholder("No function to show", cx).into_any_element();
    };
    let pass = ws.pass(first);
    let pass_name = first.snapshots[pass].pass.clone();
    let key = ((!ws.all).then_some(first.id), pass, ws.generation);
    // the IR of every shown function at the pass, by function id
    let irs: HashMap<u32, (String, Ir)> = functions
        .iter()
        .map(|f| {
            let ir = f.ir_at(pass.min(f.snapshots.len() - 1)).clone();
            (f.id, (f.name.clone(), ir))
        })
        .collect();
    let order: Vec<u32> = functions.iter().map(|f| f.id).collect();
    let layout = match &ws.cfg.cache {
        Some((k, layout)) if *k == key => layout.clone(),
        cached => {
            let other_function = cached.as_ref().is_none_or(|(k, _)| k.0 != key.0);
            let shown: Vec<(u32, &str, &Ir)> = order
                .iter()
                .map(|id| (*id, irs[id].0.as_str(), &irs[id].1))
                .collect();
            let layout = Rc::new(program(&shown, char_width(window, cx)));
            ws.cfg.cache = Some((key, layout.clone()));
            ws.cfg.pending_fit |= other_function;
            layout
        }
    };
    if ws.cfg.pending_fit && ws.cfg.viewport.get().width > px(0.) {
        ws.cfg.pending_fit = false;
        ws.cfg.fit(&layout);
    }
    let (zoom, pan) = (ws.cfg.zoom, ws.cfg.pan);
    let theme = cx.theme();
    let (fg, muted, border, bg, header_bg) = (
        theme.foreground,
        theme.muted_foreground,
        theme.border,
        theme.background,
        theme.secondary,
    );
    let (edge_color, back_color, hover_bg) =
        (theme.muted_foreground, theme.magenta, theme.list_hover);
    let hovered = ws.links().hovered;
    let active = ws.links().active;
    let at = move |(x, y): (f32, f32)| point(px(x * zoom) + pan.x, px(y * zoom) + pan.y);

    let edges = layout.edges.clone();
    let viewport = ws.cfg.viewport.clone();
    let graph = canvas(
        move |bounds, window, _| {
            if viewport.replace(bounds.size) != bounds.size {
                // a fit waiting for the pane's size can happen now
                window.refresh();
            }
        },
        move |bounds, _, window, _| {
            let shift = |p: (f32, f32)| at(p) + bounds.origin;
            for edge in &edges {
                let color = if edge.back || edge.call {
                    back_color
                } else {
                    edge_color
                };
                let mut path = PathBuilder::stroke(px((1.25 * zoom).max(0.75)));
                if edge.call {
                    path = path.dash_array(&[px(5. * zoom), px(4. * zoom)]);
                }
                path.move_to(shift(edge.points[0]));
                for c in edge.points[1..].as_chunks::<3>().0 {
                    path.cubic_bezier_to(shift(c[2]), shift(c[0]), shift(c[1]));
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, color);
                }
                // the arrow head, from the end of the curve to the tip
                let end = edge.points[edge.points.len() - 1];
                let (dx, dy) = (edge.tip.0 - end.0, edge.tip.1 - end.1);
                let len = (dx * dx + dy * dy).sqrt().max(0.001);
                let (ux, uy) = (dx / len, dy / len);
                let half = 4.;
                let base = (edge.tip.0 - ux * 8., edge.tip.1 - uy * 8.);
                let mut arrow = PathBuilder::fill();
                arrow.add_polygon(
                    &[
                        shift(edge.tip),
                        shift((base.0 - uy * half, base.1 + ux * half)),
                        shift((base.0 + uy * half, base.1 - ux * half)),
                    ],
                    true,
                );
                if let Ok(arrow) = arrow.build() {
                    window.paint_path(arrow, color);
                }
            }
        },
    )
    .absolute()
    .size_full();

    let workspace = cx.entity().downgrade();
    let titles = layout.titles.iter().map(|title| {
        let origin = at(title.origin);
        div()
            .absolute()
            .left(origin.x)
            .top(origin.y)
            .text_size(px(13. * zoom))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(fg)
            .whitespace_nowrap()
            .child(title.text.clone())
    });
    let titles: Vec<_> = titles.collect();
    let nodes = layout.nodes.iter().map(|node| {
        let ir = &irs[&node.func].1;
        let rows = &ir.rows[node.rows.0..=node.rows.1];
        let origin = at(node.origin);
        div()
            .absolute()
            .left(origin.x)
            .top(origin.y)
            .w(px(node.size.0 * zoom))
            .h(px(node.size.1 * zoom))
            .border_1()
            .border_color(border)
            .bg(bg)
            .overflow_hidden()
            .py(px(PAD * zoom))
            .text_size(px(FONT * zoom))
            .line_height(px(LINE * zoom))
            .children(rows.iter().enumerate().map(|(i, row)| {
                let indent = "  ".repeat(row.indent.saturating_sub(1) as usize);
                let text = format!("{indent}{}", row.text);
                let highlights: Vec<_> = purple_garden_ir::highlight_row(&row.text)
                    .into_iter()
                    .map(|(r, c)| (r.start + indent.len()..r.end + indent.len(), c))
                    .collect();
                let pos = row.pos.map(|pos| (node.func, pos));
                let workspace = workspace.clone();
                let is_hovered = pos.is_some() && pos == hovered;
                let is_active = pos.is_some_and(|p| active.contains(&p));
                div()
                    .id(SharedString::from(format!(
                        "cfg-{}-{}-{i}",
                        node.func, node.block
                    )))
                    .px(px(PAD * zoom))
                    .whitespace_nowrap()
                    .when(i == 0, |d| d.bg(header_bg))
                    .when(is_active, |d| d.bg(hover_bg.opacity(0.5)))
                    .when(is_hovered, |d| d.bg(hover_bg))
                    .text_color(fg)
                    .child(StyledText::new(text).with_highlights(styled(&highlights, cx)))
                    .on_hover(move |h, _, cx| {
                        if let Some(workspace) = workspace.upgrade() {
                            workspace.update(cx, |ws, cx| ws.hover_row(pos, *h, cx));
                        }
                    })
            }))
    });
    let nodes: Vec<_> = nodes.collect();

    let fit_layout = layout.clone();
    let fit = ui::button("zoom-fit", "fit", cx).on_click(cx.listener(move |ws, _, _, cx| {
        ws.cfg.fit(&fit_layout);
        cx.notify();
    }));
    let zoom_button = |id: &'static str, label: &'static str, factor: Option<f32>| {
        ui::button(id, label, cx).on_click(cx.listener(move |ws, _, _, cx| {
            match factor {
                Some(f) => ws.cfg.zoom = (ws.cfg.zoom * f).clamp(0.2, 4.),
                None => {
                    ws.cfg.zoom = 1.;
                    ws.cfg.pan = View::default().pan;
                }
            }
            cx.notify();
        }))
    };
    let toolbar = div()
        .absolute()
        .top_2()
        .right_2()
        .flex()
        .items_center()
        .gap_1()
        .bg(bg)
        .text_xs()
        .text_color(muted)
        .child(format!("after {pass_name}"))
        .child(zoom_button("zoom-out", "−", Some(1. / 1.25)))
        .child(
            div()
                .w(px(36.))
                .flex()
                .justify_center()
                .child(format!("{:.0}%", zoom * 100.)),
        )
        .child(zoom_button("zoom-in", "+", Some(1.25)))
        .child(zoom_button("zoom-reset", "100%", None))
        .child(fit);

    div()
        .id("cfg")
        .relative()
        .size_full()
        .overflow_hidden()
        .font(mono(cx))
        .cursor(if ws.cfg.drag.is_some() {
            CursorStyle::ClosedHand
        } else {
            CursorStyle::OpenHand
        })
        .on_scroll(cx.listener(|ws, e: &ScrollEvent, _, cx| {
            let delta = e.delta.pixel_delta(px(16.));
            if e.modifiers.platform || e.modifiers.control {
                let factor = (1. + f32::from(delta.y) / 300.).clamp(0.5, 2.);
                ws.cfg.zoom = (ws.cfg.zoom * factor).clamp(0.2, 4.);
            } else {
                ws.cfg.pan += delta;
            }
            cx.notify();
        }))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                ws.cfg.drag = Some((e.position, ws.cfg.pan));
                cx.notify();
            }),
        )
        .on_mouse_move(cx.listener(|ws, e: &MouseMoveEvent, _, cx| {
            if let Some((start, pan)) = ws.cfg.drag {
                if e.pressed_button != Some(MouseButton::Left) {
                    ws.cfg.drag = None;
                } else {
                    ws.cfg.pan = pan + (e.position - start);
                }
                cx.notify();
            }
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|ws, _, _, cx| {
                ws.cfg.drag = None;
                cx.notify();
            }),
        )
        .child(graph)
        .children(titles)
        .children(nodes)
        .child(toolbar)
        .into_any_element()
}
