//! The window: the editor next to the views of every compiler stage, recompiled as you type.

use std::{collections::HashSet, path::PathBuf, rc::Rc, time::Duration};

use gpui_kit::{
    component::{
        ActiveTheme as _, Theme, ThemeMode,
        highlighter::{Diagnostic as EditorDiagnostic, DiagnosticSeverity},
        input::{
            Editor, EditorState, InputEvent, RangeDecoration, RangeDecorationCollection,
            RangeDecorationStyle, RopeExt as _,
        },
        resizable::{h_resizable, resizable_panel},
    },
    prelude::FluentBuilder as _,
    *,
};
use purple_garden_inspect::{Diagnostic, Function, Inspection};

use crate::{
    editor,
    process::{self, Output},
    ui,
    views::{self, Links, RowRef, mono},
};

actions!(
    purple_garden_viz,
    [
        Save, Open, Run, Export, NextPass, PrevPass, NextTab, PrevTab, Quit
    ]
);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-s", Save, None),
        KeyBinding::new("cmd-o", Open, None),
        KeyBinding::new("cmd-r", Run, None),
        KeyBinding::new("cmd-e", Export, None),
        KeyBinding::new("cmd-alt-right", NextPass, None),
        KeyBinding::new("cmd-alt-left", PrevPass, None),
        KeyBinding::new("ctrl-tab", NextTab, None),
        KeyBinding::new("ctrl-shift-tab", PrevTab, None),
        KeyBinding::new("cmd-q", Quit, None),
    ]);
    cx.on_action(|_: &Quit, cx| cx.quit());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Ir,
    Cfg,
    Registers,
    Bytecode,
    X86,
    Ast,
    Types,
    Memory,
    Output,
}

impl Tab {
    pub const ALL: [Tab; 9] = [
        Tab::Ir,
        Tab::Cfg,
        Tab::Registers,
        Tab::Bytecode,
        Tab::X86,
        Tab::Ast,
        Tab::Types,
        Tab::Memory,
        Tab::Output,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Ir => "IR",
            Tab::Cfg => "CFG",
            Tab::Registers => "Registers",
            Tab::Bytecode => "Bytecode",
            Tab::X86 => "x86-64",
            Tab::Ast => "AST",
            Tab::Types => "Types",
            Tab::Memory => "Memory",
            Tab::Output => "Output",
        }
    }

    /// Whether the tab shows the selected functions, the others show the whole program.
    pub fn per_function(self) -> bool {
        matches!(
            self,
            Tab::Ir | Tab::Cfg | Tab::Registers | Tab::Bytecode | Tab::X86
        )
    }

    /// The tab named `name`, ignoring case, for `PG_VIZ_TAB`.
    pub fn named(name: &str) -> Option<Tab> {
        Tab::ALL
            .into_iter()
            .find(|t| t.label().eq_ignore_ascii_case(name) || (name == "x86" && *t == Tab::X86))
    }
}

/// Whose register allocation the registers view shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Bytecode,
    X86,
}

impl Target {
    pub fn label(self) -> &'static str {
        match self {
            Target::Bytecode => "bytecode",
            Target::X86 => "x86-64",
        }
    }
}

#[derive(Default)]
pub struct Scroll {
    pub ir: UniformListScrollHandle,
    pub bytecode: UniformListScrollHandle,
    pub x86: UniformListScrollHandle,
    pub ast: UniformListScrollHandle,
    pub types: UniformListScrollHandle,
}

/// Byte offsets of the starts of every line of a source.
#[derive(Debug, Clone, Default)]
pub struct Lines(Vec<usize>);

impl Lines {
    pub fn new(source: &str) -> Self {
        Self(
            std::iter::once(0)
                .chain(source.match_indices('\n').map(|(i, _)| i + 1))
                .collect(),
        )
    }

    /// The 0-based line `offset` is on.
    pub fn line_of(&self, offset: usize) -> usize {
        self.0
            .partition_point(|&start| start <= offset)
            .saturating_sub(1)
    }

    /// The byte range of `line`, without its newline.
    pub fn range(&self, line: usize, len: usize) -> std::ops::Range<usize> {
        let start = self.0.get(line).copied().unwrap_or(len);
        let end = self.0.get(line + 1).map_or(len, |next| next - 1);
        start..end.max(start)
    }
}

pub struct Workspace {
    pub path: Option<PathBuf>,
    pub editor: Entity<EditorState>,
    pub source: String,
    saved: String,
    pub lines: Lines,
    /// The last inspection that got past the frontend, kept while the source has errors.
    pub inspection: Rc<Inspection>,
    pub diagnostics: Vec<Diagnostic>,
    /// Lowering, bytecode compilation or the compiler process itself failed.
    pub error: Option<String>,
    /// Bumped by every edit, results of older compiles are dropped.
    pub generation: u64,
    compiling: bool,
    compile_task: Option<Task<()>>,
    pub tab: Tab,
    func: Option<u32>,
    /// Show every function at once.
    pub all: bool,
    /// Select the function the cursor is in.
    follow_cursor: bool,
    /// The snapshot of the selected function, `None` for its last.
    pass: Option<usize>,
    pub target: Target,
    hovered: Option<RowRef>,
    cursor_line: usize,
    pub run: Option<Output>,
    pub running: bool,
    run_task: Option<Task<()>>,
    pub cfg: views::cfg::View,
    pub scroll: Scroll,
    hover_decoration: Option<RangeDecorationCollection>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
    /// Write the window to this png once compiled, then quit.
    screenshot: Option<PathBuf>,
    /// Where the last html report went, or why it didn't.
    exported: Option<Result<PathBuf, String>>,
    window: AnyWindowHandle,
}

impl Workspace {
    pub fn new(
        path: Option<PathBuf>,
        source: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| editor::new(&source, window, cx));
        let subscriptions = vec![
            cx.subscribe_in(&editor, window, |ws, editor, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let source = editor.read(cx).value().to_string();
                    if source == ws.source {
                        return;
                    }
                    ws.source = source;
                    ws.lines = Lines::new(&ws.source);
                    ws.compile(cx);
                }
            }),
            cx.observe(&editor, |ws, editor, cx| {
                let cursor = editor.read(cx).cursor();
                let line = ws.lines.line_of(cursor);
                if line != ws.cursor_line {
                    ws.cursor_line = line;
                    if ws.follow_cursor {
                        ws.func = ws.function_at(cursor);
                    }
                    cx.notify();
                }
            }),
            cx.observe_window_appearance(window, |_, window, cx| follow_appearance(window, cx)),
        ];
        let env = |name: &str| std::env::var(name).ok();
        let mut ws = Self {
            path,
            editor,
            lines: Lines::new(&source),
            saved: source.clone(),
            source,
            inspection: Rc::default(),
            diagnostics: Vec::new(),
            error: None,
            generation: 0,
            compiling: false,
            compile_task: None,
            tab: env("PG_VIZ_TAB")
                .and_then(|t| Tab::named(&t))
                .unwrap_or(Tab::Ir),
            func: None,
            all: env("PG_VIZ_FUNC").as_deref() == Some("all"),
            follow_cursor: true,
            pass: env("PG_VIZ_PASS").and_then(|p| p.parse().ok()),
            target: if env("PG_VIZ_TARGET").as_deref() == Some("x86") {
                Target::X86
            } else {
                Target::Bytecode
            },
            hovered: None,
            cursor_line: 0,
            run: None,
            running: false,
            run_task: None,
            cfg: views::cfg::View::default(),
            scroll: Scroll::default(),
            hover_decoration: None,
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
            screenshot: env("PG_VIZ_SCREENSHOT").map(PathBuf::from),
            exported: None,
            window: window.window_handle(),
        };
        ws.compile(cx);
        if env("PG_VIZ_RUN").is_some() {
            ws.run(&Run, window, cx);
        }
        ws
    }

    /// The function the views show.
    pub fn function(&self) -> Option<&Function> {
        let functions = &self.inspection.functions;
        let by_name = |name: &str| functions.iter().find(|f| f.name == name);
        self.func
            .and_then(|id| functions.iter().find(|f| f.id == id))
            .or_else(|| functions.iter().find(|f| !f.unused && f.name != "entry"))
            .or_else(|| by_name("entry"))
    }

    /// The function whose source the byte `offset` is in, the entry for top level code.
    fn function_at(&self, offset: usize) -> Option<u32> {
        self.inspection
            .functions
            .iter()
            .filter(|f| f.name != "entry")
            .filter(|f| {
                let spans = f.snapshots[0]
                    .ir
                    .iter()
                    .flat_map(|ir| &ir.rows)
                    .filter_map(|r| r.span);
                let end = spans.max().unwrap_or(f.span) as usize;
                (f.span as usize..=end).contains(&offset)
            })
            .max_by_key(|f| f.span)
            .or_else(|| self.inspection.functions.iter().find(|f| f.name == "entry"))
            .map(|f| f.id)
    }

    /// The functions the views show: the selected one or, with [`Workspace::all`], every one.
    pub fn shown(&self) -> Vec<&Function> {
        if self.all {
            self.inspection.functions.iter().collect()
        } else {
            self.function().into_iter().collect()
        }
    }

    /// The snapshot of `f` the views show.
    pub fn pass(&self, f: &Function) -> usize {
        self.pass.unwrap_or(usize::MAX).min(f.snapshots.len() - 1)
    }

    /// The rows linked to the cursor and the hovered one.
    pub fn links(&self) -> Links {
        let active = self
            .shown()
            .into_iter()
            .flat_map(|f| {
                f.final_ir()
                    .rows
                    .iter()
                    .filter(|r| {
                        r.span
                            .is_some_and(|s| self.lines.line_of(s as usize) == self.cursor_line)
                    })
                    .filter_map(move |r| Some((f.id, r.pos?)))
            })
            .collect::<HashSet<_>>();
        Links {
            active: Rc::new(active),
            hovered: self.hovered,
        }
    }

    pub fn hover_row(&mut self, row: Option<RowRef>, hovered: bool, cx: &mut Context<Self>) {
        let next = if hovered {
            row
        } else if self.hovered == row {
            None
        } else {
            self.hovered
        };
        if next == self.hovered {
            return;
        }
        self.hovered = next;
        // the source line the row was lowered from
        let span = next.and_then(|(func, row)| {
            self.inspection
                .functions
                .iter()
                .find(|f| f.id == func)?
                .final_ir()
                .rows
                .iter()
                .find(|r| r.pos == Some(row) && r.span.is_some())?
                .span
        });
        let decorations: Vec<_> = span
            .map(|span| {
                let line = self.lines.line_of(span as usize);
                RangeDecoration::new(self.lines.range(line, self.source.len()))
                    .with_style(RangeDecorationStyle::Fill)
                    .with_color(cx.theme().yellow.opacity(0.22))
            })
            .into_iter()
            .collect();
        match &self.hover_decoration {
            Some(collection) => collection.set(decorations, cx),
            None => {
                self.hover_decoration = Some(self.editor.update(cx, |e, cx| {
                    e.create_range_decorations_collection(decorations, cx)
                }));
            }
        }
        cx.notify();
    }

    pub fn select_pass(&mut self, pass: usize, cx: &mut Context<Self>) {
        self.pass = Some(pass);
        cx.notify();
    }

    fn step_pass(&mut self, by: isize, cx: &mut Context<Self>) {
        let Some(f) = self.function() else { return };
        let last = f.snapshots.len() - 1;
        let pass = self.pass(f).saturating_add_signed(by).min(last);
        self.select_pass(pass, cx);
    }

    pub fn select_target(&mut self, target: Target, cx: &mut Context<Self>) {
        self.target = target;
        cx.notify();
    }

    fn select_function(&mut self, id: Option<u32>, cx: &mut Context<Self>) {
        self.all = id.is_none();
        if id.is_some() {
            self.func = id;
        }
        // the cursor takes over again once it moves into another function
        self.follow_cursor = true;
        self.hovered = None;
        cx.notify();
    }

    fn select_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        cx.notify();
    }

    fn step_tab(&mut self, by: isize, cx: &mut Context<Self>) {
        let at = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
        let tab = Tab::ALL[(at as isize + by).rem_euclid(Tab::ALL.len() as isize) as usize];
        self.select_tab(tab, cx);
    }

    /// Move the editor's cursor to `offset`.
    pub fn jump_to(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
        let position = editor::position(&self.source, offset);
        self.editor.update(cx, |e, cx| {
            e.set_cursor_position(position, window, cx);
            e.focus(window, cx);
        });
    }

    /// Recompile the source once typing pauses.
    fn compile(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let source = self.source.clone();
        let name = self.file_name();
        self.compiling = true;
        self.compile_task = Some(cx.spawn(async move |ws, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(120))
                .await;
            let result = cx
                .background_spawn(async move { process::inspect(&name, &source) })
                .await;
            let _ = ws.update(cx, |ws, cx| {
                if ws.generation == generation {
                    ws.compiled(result, cx);
                }
            });
        }));
        cx.notify();
    }

    fn compiled(&mut self, result: Result<Inspection, String>, cx: &mut Context<Self>) {
        self.compiling = false;
        match result {
            Ok(inspection) => {
                self.diagnostics = inspection.diagnostics.clone();
                self.error = inspection.error.clone();
                if !inspection.functions.is_empty() || inspection.diagnostics.is_empty() {
                    self.inspection = Rc::new(inspection);
                    if self.generation == 1
                        && let Ok(name) = std::env::var("PG_VIZ_FUNC")
                    {
                        self.func = self
                            .inspection
                            .functions
                            .iter()
                            .find(|f| f.name == name)
                            .map(|f| f.id);
                        self.follow_cursor = false;
                    } else if self.follow_cursor {
                        let cursor = self.editor.read(cx).cursor();
                        self.func = self.function_at(cursor);
                    }
                } else {
                    let mut stale = (*self.inspection).clone();
                    stale.diagnostics = self.diagnostics.clone();
                    self.inspection = Rc::new(stale);
                }
            }
            Err(error) => self.error = Some(error),
        }
        self.publish_diagnostics(cx);
        if self.generation == 1
            && let Some(row) = std::env::var("PG_VIZ_HOVER")
                .ok()
                .and_then(|r| r.parse().ok())
            && let Some(f) = self.function().map(|f| f.id)
        {
            self.hover_row(Some((f, row)), true, cx);
        }
        self.maybe_screenshot(cx);
        cx.notify();
    }

    /// Takes the `PG_VIZ_SCREENSHOT` once nothing is compiling or running anymore.
    fn maybe_screenshot(&mut self, cx: &mut Context<Self>) {
        if !self.compiling
            && !self.running
            && let Some(path) = self.screenshot.take()
        {
            screenshot(path, self.window, cx);
        }
    }

    /// Hands the diagnostics to the editor, which underlines them and shows their message on hover.
    fn publish_diagnostics(&mut self, cx: &mut Context<Self>) {
        let source = &self.source;
        let mut ranges: Vec<_> = self
            .diagnostics
            .iter()
            .map(|d| {
                // a point, like one at the end of a line, underlines the character before it
                let end = source.ceil_char_boundary((d.start + d.len.max(1)).min(source.len()));
                let start = source.floor_char_boundary(d.start.min(end.saturating_sub(1)));
                (start..end, d.message.clone())
            })
            .collect();
        // the set is a sum tree over offsets, parse and type errors don't arrive in order
        ranges.sort_by_key(|(range, _)| range.start);
        self.editor.update(cx, |e, cx| {
            let text = e.text().clone();
            if let Some(set) = e.diagnostics_mut() {
                // the set resolves positions against its own copy of the text
                set.reset(&text);
                set.extend(ranges.into_iter().map(|(range, message)| {
                    EditorDiagnostic::new(
                        text.offset_to_position(range.start)..text.offset_to_position(range.end),
                        message,
                    )
                    .with_severity(DiagnosticSeverity::Error)
                }));
            }
            cx.notify();
        });
    }

    fn run(&mut self, _: &Run, _: &mut Window, cx: &mut Context<Self>) {
        let source = self.source.clone();
        let name = self.file_name();
        self.running = true;
        self.tab = Tab::Output;
        self.run_task = Some(cx.spawn(async move |ws, cx| {
            let out = cx
                .background_spawn(async move {
                    process::call(process::RUN, &name, &source, Duration::from_secs(10))
                })
                .await;
            let _ = ws.update(cx, |ws, cx| {
                ws.running = false;
                ws.run = Some(out.unwrap_or_else(|e| Output {
                    stderr: e.to_string(),
                    ..Output::default()
                }));
                ws.maybe_screenshot(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else {
            let paths = cx.prompt_for_new_path(
                &std::env::current_dir().unwrap_or_default(),
                Some("main.garden"),
            );
            cx.spawn_in(window, async move |ws, cx| {
                if let Ok(Ok(Some(path))) = paths.await {
                    let _ = ws.update_in(cx, |ws, window, cx| {
                        ws.path = Some(path);
                        ws.save(&Save, window, cx);
                    });
                }
            })
            .detach();
            return;
        };
        match std::fs::write(&path, &self.source) {
            Ok(()) => self.saved = self.source.clone(),
            Err(e) => self.error = Some(format!("saving {}: {e}", path.display())),
        }
        cx.notify();
    }

    fn open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |ws, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = ws.update_in(cx, |ws, window, cx| match std::fs::read_to_string(&path) {
                Ok(source) => {
                    ws.editor
                        .update(cx, |e, cx| e.set_value(source.clone(), window, cx));
                    ws.path = Some(path);
                    ws.saved = source.clone();
                    ws.source = source;
                    ws.lines = Lines::new(&ws.source);
                    ws.func = None;
                    ws.pass = None;
                    ws.compile(cx);
                }
                Err(e) => {
                    ws.error = Some(format!("opening {}: {e}", path.display()));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The name of the file being edited, diagnostics and reports are rendered with it.
    pub fn file_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or("untitled.garden".to_string(), |n| {
                n.to_string_lossy().into_owned()
            })
    }

    fn title(&self) -> String {
        let dirty = if self.source != self.saved { " *" } else { "" };
        format!("{}{dirty}", self.file_name())
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let status = if self.compiling {
            "compiling…".to_string()
        } else if let Some(error) = &self.error {
            error.lines().next().unwrap_or("compiler error").to_string()
        } else if !self.diagnostics.is_empty() {
            let n = self.diagnostics.len();
            format!(
                "{n} error{}, showing the last good compile",
                if n == 1 { "" } else { "s" }
            )
        } else {
            let total: u64 = self.inspection.phases.iter().map(|p| p.micros).sum();
            format!("compiled in {}", views::memory::duration(total))
        };
        let bad = self.error.is_some() || !self.diagnostics.is_empty();
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .h(px(32.))
            .px_3()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap_3()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.title()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(if bad {
                                theme.danger
                            } else {
                                theme.muted_foreground
                            })
                            .child(status),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        ui::button("export", "export html  ⌘E", cx).on_click(
                            cx.listener(|ws, _, window, cx| ws.export(&Export, window, cx)),
                        ),
                    )
                    .child(
                        ui::button(
                            "run",
                            if self.running {
                                "running…"
                            } else {
                                "run  ⌘R"
                            },
                            cx,
                        )
                        .on_click(cx.listener(|ws, _, window, cx| ws.run(&Run, window, cx))),
                    ),
            )
    }

    /// Writes a self-contained html report of the source as it is now, see
    /// [`purple_garden_inspect::report`].
    fn export(&mut self, _: &Export, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.file_name();
        let source = self.source.clone();
        let dir = self
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let suggested = std::path::Path::new(&name)
            .with_extension("html")
            .to_string_lossy()
            .into_owned();
        let path = cx.prompt_for_new_path(&dir, Some(&suggested));
        cx.spawn_in(window, async move |ws, cx| {
            let Ok(Ok(Some(path))) = path.await else {
                return;
            };
            // compiled anew, the shown inspection may be the last good one of an older source
            let written = cx
                .background_spawn(async move {
                    let inspection = process::inspect(&name, &source)?;
                    let html = purple_garden_inspect::report::html(&name, &source, &inspection);
                    std::fs::write(&path, html)
                        .map(|()| path)
                        .map_err(|e| e.to_string())
                })
                .await;
            let _ = ws.update(cx, |ws, cx| {
                ws.exported = Some(written);
                cx.notify();
            });
        })
        .detach();
    }

    fn render_functions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let selected = (!self.all).then(|| self.function().map(|f| f.id)).flatten();
        let (muted, border) = (theme.muted_foreground, theme.border);
        let all = ui::item("all", self.all, cx)
            .child("all")
            .on_click(cx.listener(|ws, _, _, cx| ws.select_function(None, cx)));
        ui::bar(cx)
            .id("functions")
            .gap_0()
            .px_0()
            .overflow_x_scroll()
            .font(mono(cx))
            .text_color(theme.foreground)
            .child(all.h_full())
            .child(div().w(px(1.)).h_full().bg(border))
            .children(self.inspection.functions.iter().map(|f| {
                let id = f.id;
                ui::item(format!("function-{id}"), selected == Some(id), cx)
                    .h_full()
                    .gap(px(6.))
                    .when(f.unused, |d| d.text_color(muted))
                    .child(ui::marker(f.x86.is_ok(), f.unused, cx))
                    .child(f.name.clone())
                    .on_click(cx.listener(move |ws, _, _, cx| ws.select_function(Some(id), cx)))
            }))
            .child(
                div()
                    .ml_auto()
                    .px_3()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_color(muted)
                    .child(ui::marker(true, false, cx))
                    .child("x86-64")
                    .child(ui::marker(false, false, cx).ml_2())
                    .child("bytecode"),
            )
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let danger = theme.danger;
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(px(32.))
            .px_1()
            .border_b_1()
            .border_color(theme.border)
            .text_sm()
            .children(Tab::ALL.map(|tab| {
                let errors = match tab {
                    Tab::Output if !self.diagnostics.is_empty() => Some(self.diagnostics.len()),
                    _ => None,
                };
                ui::tab(tab.label(), tab.label(), tab == self.tab, cx)
                    .when_some(errors, |d, n| {
                        d.child(div().text_xs().text_color(danger).child(format!("({n})")))
                    })
                    .on_click(cx.listener(move |ws, _, _, cx| ws.select_tab(tab, cx)))
            }))
    }

    /// The line at the bottom: where the cursor is and what the views show.
    fn render_status(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let cursor = self.editor.read(cx).cursor();
        let line = self.lines.line_of(cursor);
        let column = cursor - self.lines.range(line, self.source.len()).start;
        let mut parts = vec![format!("{}:{}", line + 1, column + 1)];
        match &self.exported {
            Some(Ok(path)) => parts.push(format!("exported {}", path.display())),
            Some(Err(e)) => parts.push(format!("export failed: {e}")),
            None => {}
        }
        if self.tab.per_function() {
            if self.all {
                parts.push(format!("all {} functions", self.inspection.functions.len()));
            } else if let Some(f) = self.function() {
                parts.push(format!("f{} {}", f.id, f.name));
            }
            if let Some(f) = self.shown().first() {
                let pass = self.pass(f);
                parts.push(format!(
                    "after {} ({}/{})",
                    f.snapshots[pass].pass,
                    pass + 1,
                    f.snapshots.len()
                ));
            }
            if let Some((func, row)) = self.hovered {
                parts.push(format!("f{func} row {row}"));
            }
        }
        ui::bar(cx)
            .border_b_0()
            .border_t_1()
            .h(px(24.))
            .font(mono(cx))
            .gap_4()
            .children(parts)
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title(&format!("{} — purple garden", self.title()));
        let content = match self.tab {
            Tab::Ir => views::ir::render(self, cx),
            Tab::Cfg => views::cfg::render(self, window, cx),
            Tab::Registers => views::registers::render(self, cx),
            Tab::Bytecode => views::code::render_bytecode(self, cx),
            Tab::X86 => views::code::render_x86(self, cx),
            Tab::Ast => {
                let ast = self.inspection.ast.clone();
                views::code::render_text("ast", &ast, "No AST", cx, &self.scroll.ast)
            }
            Tab::Types => {
                let types = format!(
                    "# signatures\n{}\n# every value\n{}",
                    self.inspection.types.trim_end(),
                    self.inspection.typed_ast
                );
                views::code::render_text("types", &types, "No types", cx, &self.scroll.types)
            }
            Tab::Memory => views::memory::render(self, cx),
            Tab::Output => views::output::render(self, cx),
        };
        let (background, foreground, border) = (
            cx.theme().background,
            cx.theme().foreground,
            cx.theme().border,
        );
        div()
            .track_focus(&self.focus)
            .key_context("Workspace")
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::run))
            .on_action(cx.listener(Self::export))
            .on_action(cx.listener(|ws, _: &NextPass, _, cx| ws.step_pass(1, cx)))
            .on_action(cx.listener(|ws, _: &PrevPass, _, cx| ws.step_pass(-1, cx)))
            .on_action(cx.listener(|ws, _: &NextTab, _, cx| ws.step_tab(1, cx)))
            .on_action(cx.listener(|ws, _: &PrevTab, _, cx| ws.step_tab(-1, cx)))
            .size_full()
            .flex()
            .flex_col()
            .bg(background)
            .text_color(foreground)
            .child(self.render_header(cx))
            .child(
                div().flex_1().min_h_0().child(
                    h_resizable("split")
                        .child(
                            resizable_panel()
                                .size(px(560.))
                                .child(Editor::new(&self.editor).bordered(false).h_full()),
                        )
                        .child(
                            resizable_panel().child(
                                div()
                                    .size_full()
                                    .flex()
                                    .flex_col()
                                    .border_l_1()
                                    .border_color(border)
                                    .child(self.render_tabs(cx))
                                    .when(self.tab.per_function(), |d| {
                                        d.child(self.render_functions(cx))
                                    })
                                    .child(div().flex_1().min_h_0().child(content)),
                            ),
                        ),
                ),
            )
            .child(self.render_status(cx))
    }
}

/// Light or dark like the system, unless `PG_VIZ_THEME` says otherwise.
pub fn follow_appearance(window: &mut Window, cx: &mut App) {
    let dark = match std::env::var("PG_VIZ_THEME").as_deref() {
        Ok("dark") => true,
        Ok("light") => false,
        _ => matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ),
    };
    Theme::change(
        if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
        Some(window),
        cx,
    );
    ui::flatten_theme(cx);
    window.refresh();
}

/// Renders `window` to the png at `path` once layout settled, then quits.
#[cfg(feature = "screenshot")]
fn screenshot(path: PathBuf, window: AnyWindowHandle, cx: &mut Context<Workspace>) {
    cx.spawn(async move |_, cx| {
        cx.background_executor()
            .timer(Duration::from_millis(800))
            .await;
        let _ = cx.update(|cx| {
            // an occluded window isn't redrawn by the display link, draw the current state
            let saved = cx.update_window(window, |_, window, cx| {
                // twice: what the first frame measured, like the graph's pane, lays out the second
                window.draw(cx).clear(cx);
                window.draw(cx).clear(cx);
                window.render_to_image()
            });
            match saved {
                Ok(Ok(image)) => {
                    if let Err(e) = image.save(&path) {
                        eprintln!("saving {}: {e}", path.display());
                    }
                }
                Ok(Err(e)) | Err(e) => eprintln!("rendering the window: {e}"),
            }
            cx.quit();
        });
    })
    .detach();
}

#[cfg(not(feature = "screenshot"))]
fn screenshot(path: PathBuf, _: AnyWindowHandle, _: &mut Context<Workspace>) {
    eprintln!(
        "PG_VIZ_SCREENSHOT={} needs the `screenshot` feature",
        path.display()
    );
}

#[cfg(test)]
mod tests {
    use super::Lines;

    #[test]
    fn lines_map_offsets_and_ranges() {
        let src = "ab\n\ncd";
        let lines = Lines::new(src);
        assert_eq!(lines.line_of(0), 0);
        assert_eq!(lines.line_of(2), 0, "the newline belongs to its line");
        assert_eq!(lines.line_of(3), 1);
        assert_eq!(lines.line_of(4), 2);
        assert_eq!(lines.line_of(100), 2);
        assert_eq!(&src[lines.range(0, src.len())], "ab");
        assert_eq!(&src[lines.range(1, src.len())], "");
        assert_eq!(&src[lines.range(2, src.len())], "cd");
        assert_eq!(lines.range(9, src.len()), src.len()..src.len());
        let empty = Lines::new("");
        assert_eq!(empty.line_of(0), 0);
        assert_eq!(empty.range(0, 0), 0..0);
    }
}
