//! Diagnostics, compiler failures and the output of running the program.

use gpui_kit::{component::ActiveTheme as _, prelude::FluentBuilder as _, *};

use super::{memory::duration, mono};
use crate::{app::Workspace, ui};

pub fn render(ws: &Workspace, cx: &mut Context<Workspace>) -> AnyElement {
    let theme = cx.theme();
    let section = |title: &str| ui::heading(title.to_string(), cx);
    let pre = |text: String, color: Hsla| div().whitespace_normal().text_color(color).child(text);
    let mut body = div()
        .id("output")
        .size_full()
        .overflow_scroll()
        .px_4()
        .pb_4()
        .font(mono(cx))
        .text_xs();

    if let Some(error) = &ws.error {
        body = body
            .child(section("compiler"))
            .child(pre(error.clone(), theme.danger));
    }
    if !ws.diagnostics.is_empty() {
        body = body.child(section(&format!("{} problems", ws.diagnostics.len())));
        for (ix, d) in ws.diagnostics.iter().enumerate() {
            let start = d.start;
            body = body.child(
                div()
                    .id(("diagnostic", ix))
                    .cursor_pointer()
                    .py_1()
                    .hover(|s| s.bg(theme.list_hover))
                    .child(pre(d.rendered.trim_end().to_string(), theme.danger))
                    .on_click(cx.listener(move |ws, _, window, cx| ws.jump_to(start, window, cx))),
            );
        }
    }

    body = body.child(section("run"));
    body = match (&ws.run, ws.running) {
        (_, true) => body.child(pre("running…".to_string(), theme.muted_foreground)),
        (None, false) => body.child(pre(
            "Run the program with ⌘R, it runs in a child process killed after 10s".to_string(),
            theme.muted_foreground,
        )),
        (Some(out), false) => {
            let status = if out.timed_out {
                "killed after 10s".to_string()
            } else {
                match out.status {
                    Some(0) => "exited 0".to_string(),
                    Some(code) => format!("exited {code}"),
                    None => "killed by a signal".to_string(),
                }
            };
            let ok = out.status == Some(0);
            body.child(pre(
                format!(
                    "{status} after {}",
                    duration(out.elapsed.as_micros() as u64)
                ),
                if ok { theme.success } else { theme.danger },
            ))
            .when(!out.stdout.is_empty(), |b| {
                b.child(
                    div()
                        .pt_2()
                        .child(pre(out.stdout.clone(), theme.foreground)),
                )
            })
            .when(!out.stderr.is_empty(), |b| {
                b.child(div().pt_2().child(pre(out.stderr.clone(), theme.danger)))
            })
        }
    };
    body.into_any_element()
}
