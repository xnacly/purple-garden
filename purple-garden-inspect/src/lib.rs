//! Runs the compiler pipeline like the cli does, recording every stage on the way: diagnostics,
//! the AST, types, the IR after every optimisation pass, the bytecode with its register
//! allocation and the x86-64 machine code. The program itself isn't run.

pub mod model;
mod x86;

use std::{
    alloc::Allocator,
    collections::HashMap,
    time::{Duration, Instant},
};

use purple_garden_allocators::{
    bump::BumpAlloc, metric::MetricAlloc, page::PageAlloc, phases::Phases,
};
use purple_garden_bc as bc;
use purple_garden_frontend::{lex::Lexer, lower::Lower, parser::Parser};
use purple_garden_ir::Func;
use purple_garden_shared::{config::Config, xralloc2::Location as Loc};
use purple_garden_typecheck::Typechecker;

pub use model::*;

/// Inspect compiling `source`, read from `file`, with `config`. `config.no_jit` is ignored: functions are lowered to
/// bytecode and compiled by the x86-64 backend separately, on any host.
#[must_use]
pub fn inspect(file: &str, source: &[u8], config: &Config) -> Inspection {
    let phases = Phases::<MetricAlloc<PageAlloc>>::default();
    let mut timings = Timings::default();
    let mut out = Inspection::default();
    pipeline(
        file,
        source,
        config,
        phases.each_ref(),
        &mut timings,
        &mut out,
    );
    out.phases = phases
        .metrics()
        .into_iter()
        // the program isn't run
        .filter(|(name, _)| *name != "run")
        .map(|(name, m)| Phase {
            name: name.to_string(),
            micros: timings.get(name).as_micros() as u64,
            allocs: m.allocs,
            frees: m.frees,
            bytes: m.bytes,
            peak: m.peak,
        })
        .collect();
    out
}

/// Wall time per [`Phases`] name.
#[derive(Default)]
struct Timings(HashMap<&'static str, Duration>);

impl Timings {
    fn time<R>(&mut self, phase: &'static str, f: impl FnOnce() -> R) -> R {
        let start = Instant::now();
        let r = f();
        *self.0.entry(phase).or_default() += start.elapsed();
        r
    }

    fn get(&self, phase: &str) -> Duration {
        self.0.get(phase).copied().unwrap_or_default()
    }
}

fn diagnostic(
    d: &purple_garden_frontend::diagnostic::Diagnostic,
    file: &str,
    source: &[u8],
) -> Diagnostic {
    Diagnostic {
        start: d.primary.span.start,
        len: d.primary.span.len,
        message: d.message.clone(),
        rendered: d.render(file, source),
    }
}

fn pipeline(
    file: &str,
    source: &[u8],
    config: &Config,
    allocs: Phases<&MetricAlloc<PageAlloc>>,
    timings: &mut Timings,
    out: &mut Inspection,
) {
    let arena = BumpAlloc::new_in(allocs.parse);
    let mut scratch = BumpAlloc::new_in(allocs.scratch);
    let parse = timings.time("parse", || {
        Parser::new(Lexer::new(source), &arena, &scratch).parse_collect()
    });
    scratch.reset();
    out.diagnostics.extend(
        parse
            .diagnostics
            .iter()
            .map(|d| diagnostic(d, file, source)),
    );
    let Some(ast) = parse.ast else {
        return;
    };
    out.ast = ast.to_string();

    let types = BumpAlloc::new_in(allocs.typecheck);
    let stdlib = purple_garden_std::STD;
    let typecheck = timings.time("typecheck", || {
        Typechecker::new(&ast, &types, &scratch)
            .with_stdlib(stdlib)
            .check()
    });
    scratch.reset();
    out.types = typecheck.render_summary(&ast);
    out.typed_ast = typecheck.render_nodes(&ast);
    out.diagnostics.extend(
        typecheck
            .diagnostics
            .iter()
            .map(|d| diagnostic(d, file, source)),
    );
    if !out.diagnostics.is_empty() {
        return;
    }

    let ir_arena = BumpAlloc::new_in(allocs.lower);
    let lowered = timings.time("lower ir", || {
        Lower::new_in(&scratch)
            .with_stdlib(stdlib)
            .ir_from_types(&ast, typecheck.types, &ir_arena)
    });
    scratch.reset();
    let mut ir = match lowered {
        Ok(ir) => ir,
        Err(e) => {
            out.error = Some(e.render(file, source));
            return;
        }
    };

    let mut functions: Vec<Function> = ir
        .iter()
        .map(|f| Function {
            id: f.id.0,
            name: f.name.to_string(),
            span: f.span,
            unused: f.unused,
            snapshots: vec![Snapshot {
                pass: "lower".to_string(),
                ir: Some(snapshot(f)),
            }],
            intervals: Vec::new(),
            bc_locations: Vec::new(),
            x86: Err("not compiled".to_string()),
        })
        .collect();
    let index: HashMap<u32, usize> = functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.id, i))
        .collect();

    if config.opt >= 1 {
        timings.time("opt", || {
            purple_garden_opt::ir_observed(&mut ir, &scratch, |pass, f| {
                record(&mut functions[index[&f.id.0]], pass.name(), f);
            });
            purple_garden_opt::dce_functions(&mut ir, &scratch);
        });
        scratch.reset();
        for f in ir.iter() {
            let function = &mut functions[index[&f.id.0]];
            function.unused = f.unused;
            record(function, "dce_functions", f);
        }
    }

    let mut liveness = Vec::new();
    for f in ir.iter() {
        f.live_set_into(&mut liveness);
        functions[index[&f.id.0]].intervals = liveness
            .iter()
            .map(|&(def, last)| (def != u32::MAX).then_some((def, last)))
            .collect();
    }

    let mut cc = bc::Cc::new_in(&scratch, allocs.cc);
    cc.locations = Some(Vec::new());
    let bytecode_config = Config {
        no_jit: true,
        ..config.clone()
    };
    if let Err(e) = timings.time("bytecode", || {
        let compiled = cc.compile(&bytecode_config, &ir, allocs.jit).map(drop);
        if config.opt >= 1 {
            purple_garden_opt::bc(&mut cc.buf);
            cc.compact_nops();
        }
        compiled
    }) {
        out.error = Some(e);
        out.functions = functions;
        return;
    }

    for locations in cc.locations.take().unwrap_or_default() {
        functions[index[&locations.func.0]].bc_locations = locations
            .locations
            .into_iter()
            .map(|l| l.map(bytecode_location))
            .collect();
    }
    let (globals, lines) = bytecode(&cc);
    out.globals = globals;
    out.bytecode = lines;

    let (mut natives, codegen) = x86::compile(&ir, &cc.globals.map, cc.str_values());
    *timings.0.entry("jit").or_default() += codegen;
    for f in &mut functions {
        if let Some(native) = natives.remove(&purple_garden_ir::Id(f.id)) {
            f.x86 = native;
        } else if f.unused {
            f.x86 = Err("unused, not compiled".to_string());
        }
    }
    out.functions = functions;
}

/// The bytecode's register `r{num}`.
fn bytecode_location(loc: Loc) -> Location {
    match loc {
        Loc::Reg(reg) => Location::Reg(format!("r{}", reg.num())),
        Loc::Stack(slot) => Location::Slot(slot),
    }
}

fn bytecode<S: Allocator + Clone, A: Allocator + Clone>(
    cc: &bc::Cc<'_, S, A>,
) -> (Vec<String>, Vec<BcLine>) {
    let ops: Vec<_> = cc
        .buf
        .iter()
        .copied()
        .chain([purple_garden_runtime::op::Op::Halt])
        .collect();
    let dis = bc::dis::Disassembler::new(&ops, &cc.switch_tables, &cc.lookup_tables, cc.clone());
    let func_at: HashMap<usize, u32> = cc
        .functions
        .iter()
        .filter_map(|(id, f)| Some((f.pc()?, id.0)))
        .collect();
    let mut func = None;
    let lines = dis
        .lines()
        .into_iter()
        .map(|line| {
            let (kind, pc, text) = match line {
                bc::dis::Line::Func { pc, name } => {
                    func = func_at.get(&pc).copied();
                    (BcLineKind::Func, pc, name)
                }
                bc::dis::Line::Block { pc, label } => (BcLineKind::Block, pc, label),
                bc::dis::Line::Op { pc, text } => (BcLineKind::Op, pc, text),
            };
            let origin = (kind == BcLineKind::Op)
                .then(|| cc.origins.get(pc))
                .flatten();
            BcLine {
                kind,
                pc,
                text,
                func,
                span: origin.map(|o| o.span),
                row: origin.map(|o| o.row),
            }
        })
        .collect();
    (dis.globals(), lines)
}

/// Appends the IR after `pass`, `None` if it didn't change it.
fn record<F: Allocator>(function: &mut Function, pass: &str, f: &Func<'_, F>) {
    let ir = snapshot(f);
    let changed = function.final_ir() != &ir;
    function.snapshots.push(Snapshot {
        pass: pass.to_string(),
        ir: changed.then_some(ir),
    });
}

fn snapshot<F: Allocator>(f: &Func<'_, F>) -> Ir {
    let rows: Vec<IrRow> = f
        .rows()
        .into_iter()
        .map(|row| IrRow {
            pos: row.pos,
            indent: row.indent as u8,
            text: row.text,
            block: row.block.map(|b| b.0),
            span: row.span,
        })
        .collect();
    let blocks = f
        .blocks
        .iter()
        .filter(|b| !b.tombstone)
        .filter_map(|b| {
            let first = rows.iter().position(|r| r.block == Some(b.id.0))?;
            let last = rows.iter().rposition(|r| r.block == Some(b.id.0))?;
            let mut succs = Vec::new();
            let mut calls: Vec<u32> = b
                .instructions
                .iter()
                .filter_map(|i| match i {
                    purple_garden_ir::Instr::Call { func, .. } => Some(func.0),
                    _ => None,
                })
                .collect();
            if let Some(term) = &b.term {
                f.for_each_successor(term, |id| succs.push(id.0));
                if let purple_garden_ir::Terminator::Tail { func, .. } = term {
                    calls.push(func.0);
                }
            }
            Some(Block {
                id: b.id.0,
                succs,
                calls,
                rows: (first, last),
            })
        })
        .collect();
    Ir { rows, blocks }
}

#[cfg(test)]
mod tests;
