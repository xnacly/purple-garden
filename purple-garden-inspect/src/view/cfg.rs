//! Control flow graphs laid out like SpiderMonkey's iongraph (via `ion-layout`): loops indented
//! under their headers, back edges in their own channel. Functions sit side by side, call edges
//! lead from a calling block to the callee's entry block.

use std::collections::HashMap;

use ion_layout::core::{CFG as ION, Config, EdgeKind, NodeSpec, layout_with_config};
use serde::{Deserialize, Serialize};

use crate::Ir;

/// Padding inside a block, around its rows.
pub const PAD: f32 = 8.;
/// Height of a row of a block.
pub const LINE: f32 = 16.;
/// Height of a function's title above its graph.
pub const TITLE: f32 = 28.;
/// Space between the graphs of two functions.
const GAP: f32 = 96.;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// The function the block is part of.
    pub func: u32,
    pub block: u32,
    /// Top left corner.
    pub origin: (f32, f32),
    pub size: (f32, f32),
    /// Indices into [`Ir::rows`] of the block's first and last row.
    pub rows: (usize, usize),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    /// A cubic bezier chain, `3k + 1` points.
    pub points: Vec<(f32, f32)>,
    pub tip: (f32, f32),
    /// A loop's back edge.
    pub back: bool,
    /// From a calling block to the function it calls.
    pub call: bool,
}

/// The name of a function above its graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Title {
    pub func: u32,
    pub text: String,
    pub origin: (f32, f32),
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Layout {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub titles: Vec<Title>,
    /// Where each function's graph starts, `(func, x)`: its title and blocks are those of its own
    /// layout moved right by `x`.
    pub parts: Vec<(u32, f32)>,
    pub size: (f32, f32),
}

/// Lays out `functions` (id, name and IR) next to each other, every one under its title, with an
/// edge from every calling block to the entry block of the function it calls.
#[must_use]
pub fn program(functions: &[(u32, &str, &Ir)], char_width: f32) -> Layout {
    let mut out = Layout::default();
    let mut x = 0f32;
    let mut height = 0f32;
    // where a call into the function arrives: the top of its entry block
    let mut entries: HashMap<u32, (f32, f32)> = HashMap::new();
    let mut calls: Vec<((f32, f32), u32)> = Vec::new();
    for &(func, name, ir) in functions {
        let mut l = layout(ir, char_width);
        let text = format!("f{func} <{name}>");
        let width = l.size.0.max(text.chars().count() as f32 * char_width);
        let shift = |p: &mut (f32, f32)| {
            p.0 += x;
            p.1 += TITLE;
        };
        for node in &mut l.nodes {
            node.func = func;
            shift(&mut node.origin);
            let block = ir.blocks.iter().find(|b| b.id == node.block);
            for &callee in block.iter().flat_map(|b| &b.calls) {
                let (ox, oy) = node.origin;
                calls.push(((ox + node.size.0, oy + node.size.1 / 2.), callee));
            }
        }
        for edge in &mut l.edges {
            edge.points.iter_mut().for_each(shift);
            shift(&mut edge.tip);
        }
        if let Some(entry) = l.nodes.first() {
            entries.insert(func, (entry.origin.0 + entry.size.0 / 2., entry.origin.1));
        }
        out.titles.push(Title {
            func,
            text,
            origin: (x, 0.),
        });
        out.parts.push((func, x));
        out.nodes.extend(l.nodes);
        out.edges.extend(l.edges);
        height = height.max(TITLE + l.size.1);
        x += width + GAP;
    }
    for (from, callee) in calls {
        let Some(&to) = entries.get(&callee) else {
            continue;
        };
        // out to the right of the block, into the callee's title from above
        let reach = ((from.0 - to.0).abs() / 3.).clamp(48., 140.);
        out.edges.push(Edge {
            points: vec![
                from,
                (from.0 + reach, from.1),
                (to.0, to.1 - reach),
                (to.0, to.1 - 8.),
            ],
            tip: to,
            back: false,
            call: true,
        });
    }
    out.size = ((x - GAP).max(0.), height);
    out
}

/// Lays out the blocks of `ir`, `char_width` is the advance of the monospace font at [`FONT`].
#[must_use]
pub fn layout(ir: &Ir, char_width: f32) -> Layout {
    let index = |id: u32| ir.blocks.iter().position(|b| b.id == id);
    let specs: Vec<NodeSpec> = ir
        .blocks
        .iter()
        .map(|b| {
            let rows = &ir.rows[b.rows.0..=b.rows.1];
            let chars = rows
                .iter()
                .map(|r| r.text.chars().count() + 2 * r.indent.saturating_sub(1) as usize)
                .max()
                .unwrap_or(0);
            NodeSpec {
                width: f64::from(chars as f32 * char_width + 2. * PAD),
                height: f64::from(rows.len() as f32 * LINE + 2. * PAD),
                ..NodeSpec::default()
            }
        })
        .collect();
    let edges: Vec<(usize, usize)> = ir
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(from, b)| {
            b.succs
                .iter()
                .filter_map(move |&to| Some((from, index(to)?)))
        })
        .collect();
    if specs.is_empty() {
        return Layout::default();
    }
    // square corners on edges, like every other line
    let result = layout_with_config(&specs, &edges, Config { radius: 0., ..ION });
    let point = |p: &ion_layout::IonPoint| (p.x as f32, p.y as f32);
    Layout {
        nodes: ir
            .blocks
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let (w, h) = result.node_sizes[i];
                let c = point(&result.positions[i]);
                Node {
                    func: 0,
                    block: b.id,
                    origin: (c.0 - w as f32 / 2., c.1 - h as f32 / 2.),
                    size: (w as f32, h as f32),
                    rows: b.rows,
                }
            })
            .collect(),
        edges: result
            .routes
            .iter()
            .zip(&result.edge_kinds)
            .filter(|(_, kind)| **kind != EdgeKind::Invalid)
            .map(|(route, kind)| Edge {
                points: route.points.iter().map(point).collect(),
                tip: point(&route.arrow_tip),
                back: matches!(kind, EdgeKind::Feedback | EdgeKind::SelfLoop),
                call: false,
            })
            .collect(),
        titles: Vec::new(),
        parts: Vec::new(),
        size: (result.width as f32, result.height as f32),
    }
}

#[cfg(test)]
mod tests {
    use crate::Ir;
    use purple_garden_shared::config::Config;

    use super::{Layout, Node, layout, program};

    fn overlap(a: &Node, b: &Node) -> bool {
        a.origin.0 < b.origin.0 + b.size.0
            && b.origin.0 < a.origin.0 + a.size.0
            && a.origin.1 < b.origin.1 + b.size.1
            && b.origin.1 < a.origin.1 + a.size.1
    }

    #[test]
    fn every_block_gets_a_node_and_every_edge_a_route() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples");
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "garden") {
                continue;
            }
            let i = crate::inspect(
                "test.garden",
                &std::fs::read(&path).unwrap(),
                &Config::default(),
            );
            for f in &i.functions {
                for pass in 0..f.snapshots.len() {
                    let ir = f.ir_at(pass);
                    let l = layout(ir, 7.);
                    assert_eq!(
                        l.nodes.len(),
                        ir.blocks.len(),
                        "{} {}",
                        path.display(),
                        f.name
                    );
                    let succs: usize = ir.blocks.iter().map(|b| b.succs.len()).sum();
                    assert_eq!(l.edges.len(), succs, "{} {}", path.display(), f.name);
                    for e in &l.edges {
                        assert_eq!(e.points.len() % 3, 1, "a bezier chain");
                        assert!(e.points.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
                    }
                    for (a, na) in l.nodes.iter().enumerate() {
                        for nb in &l.nodes[a + 1..] {
                            assert!(
                                !overlap(na, nb),
                                "b{} overlaps b{} in {}",
                                na.block,
                                nb.block,
                                f.name
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn loops_get_back_edges() {
        use crate::{Block, IrRow};
        // b0 -> b1 -> b2 -> b1, b2 -> b3
        let row = |text: &str, block| IrRow {
            pos: Some(0),
            indent: 1,
            text: text.to_string(),
            block: Some(block),
            span: None,
        };
        let ir = Ir {
            rows: vec![
                row("b0():", 0),
                row("b1():", 1),
                row("b2():", 2),
                row("b3():", 3),
            ],
            blocks: vec![
                Block {
                    id: 0,
                    succs: vec![1],
                    calls: vec![],
                    rows: (0, 0),
                },
                Block {
                    id: 1,
                    succs: vec![2],
                    calls: vec![],
                    rows: (1, 1),
                },
                Block {
                    id: 2,
                    succs: vec![1, 3],
                    calls: vec![],
                    rows: (2, 2),
                },
                Block {
                    id: 3,
                    succs: vec![],
                    calls: vec![],
                    rows: (3, 3),
                },
            ],
        };
        let l = layout(&ir, 7.);
        assert_eq!(l.edges.len(), 4);
        assert_eq!(
            l.edges.iter().filter(|e| e.back).count(),
            1,
            "{:#?}",
            l.edges
        );
    }

    #[test]
    fn self_tail_calls_have_no_edges() {
        let i = crate::inspect("test.garden", b"fn count(n:Int acc:Int) Int { match { n == 0 { acc } { count(n - 1 acc + 1) } } }\ncount(3 0)",
            &Config::default(),
        );
        let f = i.functions.iter().find(|f| f.name == "count").unwrap();
        let l = layout(f.final_ir(), 7.);
        assert!(
            l.edges.iter().all(|e| !e.back),
            "the tail call is a terminator, not an edge"
        );
    }

    #[test]
    fn programs_place_functions_side_by_side_with_call_edges() {
        let i = crate::inspect(
            "test.garden",
            include_bytes!("../../../examples/fib.garden"),
            &Config::default(),
        );
        let shown: Vec<_> = i
            .functions
            .iter()
            .map(|f| (f.id, f.name.as_str(), f.final_ir()))
            .collect();
        let l = program(&shown, 7.);
        assert_eq!(l.titles.len(), shown.len());
        let blocks: usize = shown.iter().map(|(_, _, ir)| ir.blocks.len()).sum();
        assert_eq!(l.nodes.len(), blocks);
        // clusters don't overlap: every function's nodes stay right of the previous title
        for pair in l.titles.windows(2) {
            assert!(pair[1].origin.0 > pair[0].origin.0);
            let (prev, next) = (pair[0].func, pair[1].func);
            let prev_right = l
                .nodes
                .iter()
                .filter(|n| n.func == prev)
                .map(|n| n.origin.0 + n.size.0)
                .fold(0f32, f32::max);
            assert!(
                l.nodes
                    .iter()
                    .filter(|n| n.func == next)
                    .all(|n| n.origin.0 > prev_right)
            );
        }
        let calls: usize = shown
            .iter()
            .flat_map(|(_, _, ir)| &ir.blocks)
            .map(|b| b.calls.len())
            .sum();
        assert!(calls > 0);
        assert_eq!(l.edges.iter().filter(|e| e.call).count(), calls);
        for (a, na) in l.nodes.iter().enumerate() {
            for nb in &l.nodes[a + 1..] {
                assert!(!overlap(na, nb), "{na:?} overlaps {nb:?}");
            }
        }
        assert!(l.nodes.iter().all(|n| n.origin.1 >= super::TITLE));
        assert_eq!(program(&[], 7.), Layout::default());
    }

    #[test]
    fn empty_functions_lay_out_to_nothing() {
        let l = layout(
            &Ir {
                rows: vec![],
                blocks: vec![],
            },
            7.,
        );
        assert_eq!(l, Layout::default());
    }
}
