//! A self-contained html report of an [`Inspection`]: one file, no server and no network, its
//! views drawn by plain javascript from the data embedded as json.
//!
//! Everything a view shows is computed here, by [`crate::view`], the page only draws it.

use std::collections::HashMap;

use purple_garden_shared::ansi::Class;
use serde::{Serialize, Serializer, ser::SerializeTuple};

use crate::{
    Diagnostic, Function, Inspection, Phase,
    view::{
        cfg::{self, Layout},
        listing::{Change, Line, Tone, bytecode_lines, changes, plain, text_lines, x86_lines},
        registers::{Bar, lanes, pressure},
        source::garden_tokens,
    },
};

/// Advance of a character in the graphs' monospace font, the page scales its font to match.
pub const CHAR_WIDTH: f32 = 6.6;

const TEMPLATE: &str = include_str!("report/report.html");
const STYLE: &str = include_str!("report/report.css");
const SCRIPT: &str = include_str!("report/report.js");

/// The report of `inspection`, compiled from `source` read from `file`.
#[must_use]
pub fn html(file: &str, source: &str, inspection: &Inspection) -> String {
    let data = serde_json::to_string(&Report::new(file, source, inspection))
        .expect("the report serializes");
    // the json sits in a script element, `</script>` in a string must not end it
    let data = data
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026");
    fill(
        TEMPLATE,
        &[
            ("title", &escape(file)),
            ("style", STYLE),
            ("script", SCRIPT),
            ("data", &data),
        ],
    )
}

/// `template` with every `{{name}}` replaced by its value, in one pass: a value containing a
/// placeholder, like a file named `{{data}}`, stays as it is.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out =
        String::with_capacity(template.len() + values.iter().map(|(_, v)| v.len()).sum::<usize>());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}").and_then(|end| {
            values
                .iter()
                .find(|(name, _)| *name == &after[..end])
                .map(|v| (end, v.1))
        }) {
            Some((end, value)) => {
                out.push_str(value);
                rest = &after[end + 2..];
            }
            None => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `text` safe inside html text and attributes.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A [`Line`] as a tuple: gutter, text, highlights as `[start, end, class]`, row and tone, the
/// class and tone as indices into [`Report::classes`] and [`Report::tones`].
struct CompactLine<'a>(&'a Line);

impl Serialize for CompactLine<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let line = self.0;
        let mut t = s.serialize_tuple(5)?;
        t.serialize_element(&line.gutter)?;
        t.serialize_element(&line.text)?;
        let highlights: Vec<(usize, usize, u8)> = line
            .highlights
            .iter()
            .map(|(r, c)| (r.start, r.end, *c as u8))
            .collect();
        t.serialize_element(&highlights)?;
        t.serialize_element(&line.row)?;
        t.serialize_element(&(line.tone as u8))?;
        t.end()
    }
}

#[derive(Serialize)]
struct Lines(#[serde(serialize_with = "compact")] Vec<Line>);

fn compact<S: Serializer>(lines: &[Line], s: S) -> Result<S::Ok, S::Error> {
    s.collect_seq(lines.iter().map(CompactLine))
}

/// A [`Change`] as `[kind, start, len]`, kind 0 kept, 1 removed, 2 added.
fn change(change: &Change) -> [usize; 3] {
    match *change {
        Change::Kept { new, len } => [0, new, len],
        Change::Removed { old, len } => [1, old, len],
        Change::Added { new, len } => [2, new, len],
    }
}

/// A [`Layout`] in whole pixels and arrays: nodes `[func, block, x, y, w, h, first row, last
/// row]`, edges `[kind, points as x, y, ..., tip x, tip y]` with kind 0 an edge, 1 a back edge and
/// 2 a call, titles `[text, x, y]`.
struct CompactLayout(Layout);

fn px(v: f32) -> i32 {
    v.round() as i32
}

impl CompactLayout {
    fn edges(&self) -> Vec<Vec<i32>> {
        self.0
            .edges
            .iter()
            .map(|e| {
                let kind = if e.call { 2 } else { i32::from(e.back) };
                std::iter::once(kind)
                    .chain(e.points.iter().flat_map(|p| [px(p.0), px(p.1)]))
                    .chain([px(e.tip.0), px(e.tip.1)])
                    .collect()
            })
            .collect()
    }
}

impl Serialize for CompactLayout {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let l = &self.0;
        let nodes: Vec<[i64; 8]> = l
            .nodes
            .iter()
            .map(|n| {
                [
                    i64::from(n.func),
                    i64::from(n.block),
                    i64::from(px(n.origin.0)),
                    i64::from(px(n.origin.1)),
                    i64::from(px(n.size.0)),
                    i64::from(px(n.size.1)),
                    n.rows.0 as i64,
                    n.rows.1 as i64,
                ]
            })
            .collect();
        let edges = self.edges();
        let titles: Vec<(&str, i32, i32)> = l
            .titles
            .iter()
            .map(|t| (t.text.as_str(), px(t.origin.0), px(t.origin.1)))
            .collect();
        let mut t = s.serialize_tuple(4)?;
        t.serialize_element(&nodes)?;
        t.serialize_element(&edges)?;
        t.serialize_element(&titles)?;
        t.serialize_element(&[px(l.size.0), px(l.size.1)])?;
        t.end()
    }
}

/// Distinct layouts, referred to by index: by what they were computed for, then by what they look
/// like, since passes changing a function's text often leave its graph's shape as it was.
struct Layouts {
    by_key: HashMap<Vec<(u32, usize)>, usize>,
    by_json: HashMap<String, usize>,
    values: Vec<CompactLayout>,
}

impl Layouts {
    fn get(&mut self, key: Vec<(u32, usize)>, layout: impl FnOnce() -> Layout) -> usize {
        if let Some(&i) = self.by_key.get(&key) {
            return i;
        }
        let layout = CompactLayout(layout());
        let json = serde_json::to_string(&layout).expect("layouts serialize");
        let next = self.values.len();
        let i = *self.by_json.entry(json).or_insert(next);
        if i == next {
            self.values.push(layout);
        }
        self.by_key.insert(key, i);
        i
    }
}

/// Every function's graph after a pass, side by side: each function's own layout moved right,
/// plus the calls between them.
#[derive(Serialize)]
struct Program {
    /// `(layout, x)`: the function's layout and how far right it moves.
    parts: Vec<(usize, i32)>,
    /// The call edges, like [`CompactLayout`]'s edges.
    calls: Vec<Vec<i32>>,
    size: [i32; 2],
}

/// Distinct values, referred to by index: most passes leave a function as it was.
struct Interner<K, V> {
    index: HashMap<K, usize>,
    values: Vec<V>,
}

impl<K: std::hash::Hash + Eq, V> Interner<K, V> {
    fn new() -> Self {
        Self {
            index: HashMap::new(),
            values: Vec::new(),
        }
    }

    fn get(&mut self, key: K, value: impl FnOnce() -> V) -> usize {
        *self.index.entry(key).or_insert_with(|| {
            self.values.push(value());
            self.values.len() - 1
        })
    }
}

#[derive(Serialize)]
struct Report<'a> {
    file: &'a str,
    source: &'a str,
    tokens: Vec<(usize, usize, &'static str)>,
    diagnostics: &'a [Diagnostic],
    error: Option<&'a str>,
    phases: &'a [Phase],
    /// Highlight class names, by the index lines use.
    classes: Vec<Class>,
    /// Tone names, by the index lines use.
    tones: [Tone; 5],
    ast: Lines,
    types: Lines,
    /// The pass names, the same for every function.
    passes: Vec<String>,
    functions: Vec<FunctionReport<'a>>,
    /// Distinct IR snapshots, a line per row.
    snapshots: Vec<Lines>,
    /// Every function side by side after each pass, by index into `programs`.
    program: Vec<usize>,
    programs: Vec<Program>,
    layouts: Vec<CompactLayout>,
    char_width: f32,
    line: f32,
    pad: f32,
    host_runs_x86: bool,
}

#[derive(Serialize)]
struct FunctionReport<'a> {
    id: u32,
    name: &'a str,
    unused: bool,
    native: bool,
    /// Why the function stays bytecode.
    reason: Option<&'a str>,
    /// The IR after each pass, by index into `snapshots`.
    rows: Vec<usize>,
    /// How each pass changed the IR, as `[kind, start, len]` runs, `None` if it didn't.
    changes: Vec<Option<Vec<[usize; 3]>>>,
    /// The graph after each pass, by index into `layouts`.
    cfg: Vec<usize>,
    bytecode: Lines,
    x86: Lines,
    registers: Registers,
    pressure: Vec<u32>,
    /// `(row, block)` of every block's label in the final IR.
    blocks: Vec<(u32, u32)>,
    /// `(row, byte offset into the source)` of every row lowered from the source.
    spans: Vec<(u32, u32)>,
}

#[derive(Serialize)]
struct Registers {
    bytecode: Result<Vec<(String, Vec<Bar>)>, String>,
    x86: Result<Vec<(String, Vec<Bar>)>, String>,
}

fn registers(f: &Function) -> Registers {
    let lanes_of = |locations: &[Option<crate::Location>]| {
        lanes(&f.intervals, locations)
            .into_iter()
            .map(|(location, bars)| (location.to_string(), bars))
            .collect()
    };
    Registers {
        bytecode: if f.bc_locations.is_empty() {
            Err("not lowered to bytecode, so no registers were allocated".to_string())
        } else {
            Ok(lanes_of(&f.bc_locations))
        },
        x86: match &f.x86 {
            Ok(native) => Ok(lanes_of(&native.locations)),
            Err(reason) => Err(format!("not compiled to x86-64: {reason}")),
        },
    }
}

/// The snapshot whose IR `f` shows after `pass`, what identical views share.
fn shown(f: &Function, pass: usize) -> usize {
    let pass = pass.min(f.snapshots.len() - 1);
    (0..=pass)
        .rev()
        .find(|&i| f.snapshots[i].ir.is_some())
        .unwrap_or(0)
}

impl<'a> Report<'a> {
    fn new(file: &'a str, source: &'a str, inspection: &'a Inspection) -> Self {
        let functions = &inspection.functions;
        let passes: Vec<String> = functions
            .first()
            .map(|f| f.snapshots.iter().map(|s| s.pass.clone()).collect())
            .unwrap_or_default();
        let mut snapshots = Interner::new();
        let mut layouts = Layouts {
            by_key: HashMap::new(),
            by_json: HashMap::new(),
            values: Vec::new(),
        };
        let mut programs = Interner::new();
        let reports = functions
            .iter()
            .map(|f| {
                let passes = 0..f.snapshots.len();
                let last = f.final_ir();
                FunctionReport {
                    id: f.id,
                    name: &f.name,
                    unused: f.unused,
                    native: f.x86.is_ok(),
                    reason: f.x86.as_ref().err().map(String::as_str),
                    rows: passes
                        .clone()
                        .map(|pass| {
                            let at = shown(f, pass);
                            snapshots.get((f.id, at), || Lines(plain(f, at)))
                        })
                        .collect(),
                    changes: passes
                        .clone()
                        .map(|pass| changes(f, pass).map(|c| c.iter().map(change).collect()))
                        .collect(),
                    cfg: passes
                        .map(|pass| {
                            let at = shown(f, pass);
                            layouts.get(vec![(f.id, at)], || {
                                cfg::program(&[(f.id, &f.name, f.ir_at(at))], CHAR_WIDTH)
                            })
                        })
                        .collect(),
                    bytecode: Lines(bytecode_lines(inspection, Some(&[f]))),
                    x86: Lines(x86_lines(&[f])),
                    registers: registers(f),
                    pressure: pressure(&f.intervals),
                    blocks: last
                        .rows
                        .iter()
                        .filter(|r| r.indent == 1)
                        .filter_map(|r| Some((r.pos?, r.block?)))
                        .collect(),
                    spans: last
                        .rows
                        .iter()
                        .filter_map(|r| Some((r.pos?, r.span?)))
                        .collect(),
                }
            })
            .collect();
        let program = (0..passes.len())
            .map(|pass| {
                let key: Vec<(u32, usize)> =
                    functions.iter().map(|f| (f.id, shown(f, pass))).collect();
                programs.get(key.clone(), || {
                    let shown: Vec<_> = functions
                        .iter()
                        .zip(&key)
                        .map(|(f, &(_, at))| (f.id, f.name.as_str(), f.ir_at(at)))
                        .collect();
                    let whole = cfg::program(&shown, CHAR_WIDTH);
                    let calls = CompactLayout(Layout {
                        edges: whole.edges.iter().filter(|e| e.call).cloned().collect(),
                        ..Layout::default()
                    });
                    let parts = whole
                        .parts
                        .iter()
                        .zip(&key)
                        .zip(functions)
                        .map(|((&(_, x), &(_, at)), f)| {
                            let own = layouts.get(vec![(f.id, at)], || {
                                cfg::program(&[(f.id, &f.name, f.ir_at(at))], CHAR_WIDTH)
                            });
                            (own, x.round() as i32)
                        })
                        .collect();
                    Program {
                        parts,
                        calls: calls.edges(),
                        size: [whole.size.0.round() as i32, whole.size.1.round() as i32],
                    }
                })
            })
            .collect();
        Report {
            file,
            source,
            tokens: garden_tokens(source)
                .into_iter()
                .map(|(r, name)| (r.start, r.end, name))
                .collect(),
            diagnostics: &inspection.diagnostics,
            error: inspection.error.as_deref(),
            phases: &inspection.phases,
            classes: Class::ALL.to_vec(),
            tones: Tone::ALL,
            ast: Lines(text_lines(&inspection.ast)),
            types: Lines(text_lines(&format!(
                "# signatures\n{}\n# every value\n{}",
                inspection.types.trim_end(),
                inspection.typed_ast
            ))),
            passes,
            functions: reports,
            snapshots: snapshots.values,
            program,
            programs: programs.values,
            layouts: layouts.values,
            char_width: CHAR_WIDTH,
            line: cfg::LINE,
            pad: cfg::PAD,
            host_runs_x86: cfg!(target_arch = "x86_64"),
        }
    }
}

#[cfg(test)]
mod tests {
    use purple_garden_shared::config::Config;

    use super::html;
    use crate::inspect;

    /// The json the report embeds, unescaped.
    fn data(html: &str) -> serde_json::Value {
        let start = html
            .find(r#"<script id="data" type="application/json">"#)
            .unwrap();
        let start = html[start..].find('>').unwrap() + start + 1;
        let end = html[start..].find("</script>").unwrap() + start;
        serde_json::from_str(&html[start..end]).unwrap()
    }

    #[test]
    fn reports_embed_every_view() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../examples/fib.garden"
        ))
        .unwrap();
        let i = inspect("test.garden", src.as_bytes(), &Config::default());
        let page = html("fib.garden", &src, &i);
        assert!(page.starts_with("<!doctype html>"));
        assert!(page.contains("<title>fib.garden"));
        let d = data(&page);
        assert_eq!(d["source"], src);
        let functions = d["functions"].as_array().unwrap();
        assert_eq!(functions.len(), i.functions.len());
        let passes = d["passes"].as_array().unwrap().len();
        // lowering, every pass of the pipeline, dce_functions
        assert_eq!(passes, purple_garden_opt::Pass::PIPELINE.len() + 2);
        let snapshots = d["snapshots"].as_array().unwrap().len();
        let layouts = d["layouts"].as_array().unwrap().len();
        for f in functions {
            let index = |name: &str| {
                f[name]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|i| i.as_u64().unwrap() as usize)
                    .collect::<Vec<_>>()
            };
            assert_eq!(index("rows").len(), passes);
            assert_eq!(index("cfg").len(), passes);
            assert!(index("rows").iter().all(|&i| i < snapshots));
            assert!(index("cfg").iter().all(|&i| i < layouts));
            let changes = f["changes"].as_array().unwrap();
            assert_eq!(changes.len(), passes);
            assert!(changes[0].is_null(), "lowering changes nothing");
        }
        assert_eq!(d["program"].as_array().unwrap().len(), passes);
        for program in d["programs"].as_array().unwrap() {
            for part in program["parts"].as_array().unwrap() {
                assert!((part[0].as_u64().unwrap() as usize) < layouts);
            }
        }
        // fewer snapshots than passes times functions: unchanged passes share them
        assert!(snapshots < passes * functions.len());
        let fib = functions.iter().find(|f| f["name"] == "fib").unwrap();
        assert_eq!(fib["native"], true);
        assert!(fib["registers"]["bytecode"]["Ok"].as_array().is_some());
        assert!(!fib["spans"].as_array().unwrap().is_empty());
        // a line is [gutter, text, highlights, row, tone]
        let line = &fib["bytecode"][1];
        assert_eq!(line.as_array().unwrap().len(), 5);
        let class = line[2][0][2].as_u64().unwrap() as usize;
        assert!(
            d["classes"][class].is_string(),
            "highlight classes by index: {line}"
        );
        assert_eq!(d["tones"][line[4].as_u64().unwrap() as usize], "Normal");
    }

    #[test]
    fn script_closing_tags_in_the_source_stay_inside_the_json() {
        let src = "# </script><script>alert(1)</script> & <b>\nlet x = \"</script>\"\nx";
        let i = inspect("test.garden", src.as_bytes(), &Config::default());
        let page = html("<evil>.garden", src, &i);
        assert_eq!(
            page.matches("</script>").count(),
            2,
            "only the data's and the viewer's"
        );
        assert!(page.contains("<title>&lt;evil&gt;.garden"));
        assert_eq!(data(&page)["source"], src);
    }

    #[test]
    fn placeholders_fill_once() {
        assert_eq!(
            super::fill("a {{x}} b {{y}} {{z}} {{", &[("x", "{{y}}"), ("y", "2")]),
            "a {{y}} b 2 {{z}} {{"
        );
        assert_eq!(super::fill("", &[]), "");
    }

    #[test]
    fn broken_sources_still_report() {
        let src = "fn f(a:Int) Int { a + }";
        let i = inspect("test.garden", src.as_bytes(), &Config::default());
        let d = data(&html("broken.garden", src, &i));
        assert!(!d["diagnostics"].as_array().unwrap().is_empty());
        assert!(d["functions"].as_array().unwrap().is_empty());
        assert!(d["passes"].as_array().unwrap().is_empty());
        let empty = data(&html(
            "empty.garden",
            "",
            &inspect("test.garden", b"", &Config::default()),
        ));
        assert_eq!(empty["source"], "");
    }

    #[test]
    fn every_example_reports() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples");
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "garden") {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            let i = inspect("test.garden", src.as_bytes(), &Config::default());
            let page = html(&path.display().to_string(), &src, &i);
            data(&page);
            assert!(
                page.len() < 6 << 20,
                "{} is {} bytes",
                path.display(),
                page.len()
            );
        }
    }
}
