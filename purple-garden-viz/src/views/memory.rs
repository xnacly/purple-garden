//! Time and memory every phase of the pipeline took.

use gpui_kit::{component::ActiveTheme as _, *};

use super::{mono, placeholder};
use crate::app::Workspace;

/// `bytes` in the largest unit keeping it above one.
#[must_use]
pub fn bytes(n: usize) -> String {
    match n {
        0..1024 => format!("{n} B"),
        1024..0x10_0000 => format!("{:.1} KiB", n as f64 / 1024.),
        _ => format!("{:.1} MiB", n as f64 / f64::from(1 << 20)),
    }
}

/// `micros` in ms or µs.
#[must_use]
pub fn duration(micros: u64) -> String {
    if micros >= 1000 {
        format!("{:.2} ms", micros as f64 / 1000.)
    } else {
        format!("{micros} µs")
    }
}

pub fn render(ws: &Workspace, cx: &mut Context<Workspace>) -> AnyElement {
    let phases = &ws.inspection.phases;
    if phases.is_empty() {
        return placeholder("Nothing compiled yet", cx).into_any_element();
    }
    let theme = cx.theme();
    let max_time = phases.iter().map(|p| p.micros).max().unwrap_or(1).max(1) as f32;
    let max_peak = phases.iter().map(|p| p.peak).max().unwrap_or(1).max(1) as f32;
    let bar = |fraction: f32, color: Hsla| {
        div()
            .w(px(140.))
            .h(px(8.))
            .bg(theme.muted)
            .child(div().h_full().w(px(140. * fraction)).bg(color))
    };
    let cell = |w: f32| div().w(px(w)).flex_none();
    let header = div()
        .flex()
        .gap_3()
        .py_1()
        .text_color(theme.muted_foreground)
        .border_b_1()
        .border_color(theme.border)
        .child(cell(90.).child("phase"))
        .child(cell(220.).child("wall time"))
        .child(cell(220.).child("peak live"))
        .child(cell(80.).child("allocs"))
        .child(cell(80.).child("frees"))
        .child(cell(90.).child("allocated"));
    let rows = phases.iter().map(|p| {
        div()
            .flex()
            .items_center()
            .gap_3()
            .py_1()
            .child(cell(90.).child(p.name.clone()))
            .child(
                cell(220.)
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(bar(p.micros as f32 / max_time, theme.blue))
                    .child(duration(p.micros)),
            )
            .child(
                cell(220.)
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(bar(p.peak as f32 / max_peak, theme.magenta))
                    .child(bytes(p.peak)),
            )
            .child(cell(80.).child(p.allocs.to_string()))
            .child(cell(80.).child(p.frees.to_string()))
            .child(cell(90.).child(bytes(p.bytes)))
    });
    let total: u64 = phases.iter().map(|p| p.micros).sum();
    div()
        .id("memory")
        .size_full()
        .overflow_scroll()
        .p_4()
        .font(mono(cx))
        .text_xs()
        .child(header)
        .children(rows)
        .child(
            div()
                .pt_3()
                .text_color(theme.muted_foreground)
                .child(format!(
                    "compiled in {} · run isn't measured here, see Output",
                    duration(total)
                )),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{bytes, duration};

    #[test]
    fn units() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1536), "1.5 KiB");
        assert_eq!(bytes(3 << 20), "3.0 MiB");
        assert_eq!(duration(999), "999 µs");
        assert_eq!(duration(1500), "1.50 ms");
    }
}
