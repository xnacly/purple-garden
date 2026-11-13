//! `purple-garden-viz [file.garden]`: an editor next to every stage of the compiler, recompiled as
//! you type.
//!
//! Development switches, read once at startup:
//!
//! - `PG_VIZ_TAB=cfg|ir|registers|bytecode|x86|ast|types|memory|output` opens a tab
//! - `PG_VIZ_FUNC=<name>`, `PG_VIZ_PASS=<snapshot>`, `PG_VIZ_HOVER=<row>`, `PG_VIZ_TARGET=x86`
//!   preselect what the views show
//! - `PG_VIZ_THEME=dark|light` overrides the system appearance
//! - `PG_VIZ_RUN=1` runs the program right away
//! - `PG_VIZ_SCREENSHOT=<png>` writes the window once compiled and quits, needs `--features
//!   screenshot`
mod app;
mod editor;
mod highlight;
mod process;
mod ui;
mod views;

use std::path::PathBuf;

use gpui_kit::*;

/// Shown without a file to open.
const WELCOME: &str = "import (\"io\")

#! Naive recursion, the jit compiles it to a native self call.
fn fib(n:Int) Int {
    match {
        n < 2 { n }
        { fib(n - 1) + fib(n - 2) }
    }
}

#! A tail call, optimised into a loop.
fn sum(n:Int acc:Int) Int {
    match {
        n == 0 { acc }
        { sum(n - 1 acc + n) }
    }
}

io.println(fib(20))
io.println(sum(100 0))
";

fn main() {
    let arg = std::env::args().nth(1);
    match arg.as_deref() {
        Some(process::INSPECT) => return process::serve_inspect(),
        Some(process::RUN) => return process::serve_run(),
        _ => {}
    }
    let path = arg.map(PathBuf::from);
    let source = match &path {
        Some(path) => std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("{}: {e}", path.display());
            std::process::exit(1)
        }),
        None => WELCOME.to_string(),
    };

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        app::bind_keys(cx);
        let size = std::env::var("PG_VIZ_SIZE")
            .ok()
            .and_then(|s| {
                let (w, h) = s.split_once('x')?;
                Some(size(px(w.parse().ok()?), px(h.parse().ok()?)))
            })
            .unwrap_or(size(px(1500.), px(940.)));
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size, cx))),
            ..WindowOptions::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            app::follow_appearance(window, cx);
            cx.new(|cx| app::Workspace::new(path, source, window, cx))
        })
        .expect("opening the window");
        cx.activate(true);
    });
}
