//! Typechecker-only benches
//!
//! Every `examples/*.garden` program is parsed once outside the timed loop and
//! only `Typechecker::check` is measured, so this isolates the typechecker from
//! lexing, parsing and lowering that `suite`'s `<name>_compile` folds together.
//!
//! The `synth_*` programs are generated stress inputs, each isolating one
//! typechecker path: call checking (`std_calls`, `user_calls`, `overloads`,
//! `many_fns`), slot binding (`generic_calls`, `generic_deep`), env traffic
//! (`idents`, `long_body`, `nested_match`), structural types (`deep_record`,
//! `wide_record`, `wide_record_param`, `arrays`) and recursion depth
//! (`deep_expr`).

use std::path::PathBuf;

use criterion::Criterion;
use purple_garden_allocators::bump::BumpAlloc;
use purple_garden_frontend::{ast::Ast, lex, parser};
use purple_garden_typecheck::Typechecker;

mod common;

const SYNTH_LINES: usize = 2_000;
const SYNTH_DEPTH: usize = 256;
const SYNTH_DEPTH_EXPR: usize = 1_000;
const SYNTH_DEPTH_RECORD: usize = 64;
const SYNTH_DEPTH_SCOPE: usize = 128;
const SYNTH_WIDTH: usize = 500;
const SYNTH_FNS: usize = 1_000;

fn programs() -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
        .expect("examples dir missing")
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("garden"))
        .map(|p| {
            let name = p.file_stem().unwrap().to_string_lossy().into_owned();
            let bytes = std::fs::read(&p).unwrap_or_else(|e| panic!("read {}: {}", p.display(), e));
            (name, bytes)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn synth_std_calls() -> Vec<u8> {
    let mut src = String::from("import (\"math\" \"testing\")\n");
    for i in 0..SYNTH_LINES {
        src.push_str(&format!("testing.assert(math.abs(-{i}) == {i})\n"));
    }
    src.into_bytes()
}

fn synth_user_calls() -> Vec<u8> {
    let mut src = String::from("fn add(a:Int b:Int) Int { a + b }\n");
    for i in 0..SYNTH_LINES {
        src.push_str(&format!("let v{i} = add({i} 1)\n"));
    }
    src.into_bytes()
}

fn synth_idents() -> Vec<u8> {
    let mut src = String::from("let v0 = 1\n");
    for i in 1..SYNTH_LINES {
        src.push_str(&format!("let v{i} = v{} + v0\n", i - 1));
    }
    src.into_bytes()
}

fn synth_generic_calls() -> Vec<u8> {
    let mut src = String::from("import \"opt\"\n");
    for i in 0..SYNTH_LINES {
        src.push_str(&format!("let v{i} = opt.some({i})\n"));
    }
    src.into_bytes()
}

/// One expression wrapped `SYNTH_DEPTH` times, so every level binds `T` to a one-deeper
/// `Option` chain and the return type grows with the nesting
fn synth_generic_deep() -> Vec<u8> {
    let mut src = String::from("import \"opt\"\nlet v = ");
    for _ in 0..SYNTH_DEPTH {
        src.push_str("opt.some(");
    }
    src.push('1');
    for _ in 0..SYNTH_DEPTH {
        src.push(')');
    }
    src.push('\n');
    src.into_bytes()
}

/// Left-deep `1 + 1 + ...`, recursion depth of `node` equals the term count
fn synth_deep_expr() -> Vec<u8> {
    let mut src = String::from("let v = 1");
    for _ in 1..SYNTH_DEPTH_EXPR {
        src.push_str(" + 1");
    }
    src.push('\n');
    src.into_bytes()
}

/// `{a: {a: ... 1}}` then `v.a.a...`, every level owns the full type below it
fn synth_deep_record() -> Vec<u8> {
    let mut src = String::from("let v = ");
    for _ in 0..SYNTH_DEPTH_RECORD {
        src.push_str("{a: ");
    }
    src.push('1');
    for _ in 0..SYNTH_DEPTH_RECORD {
        src.push('}');
    }
    src.push_str("\nlet w = v");
    for _ in 0..SYNTH_DEPTH_RECORD {
        src.push_str(".a");
    }
    src.push('\n');
    src.into_bytes()
}

/// One record with `SYNTH_WIDTH` fields, read back field by field, so each identifier use
/// touches the whole record type
fn synth_wide_record() -> Vec<u8> {
    let mut src = String::from("let r = {");
    for i in 0..SYNTH_WIDTH {
        src.push_str(&format!("f{i}: {i} "));
    }
    src.push_str("}\n");
    for i in 0..SYNTH_WIDTH {
        src.push_str(&format!("let x{i} = r.f{i}\n"));
    }
    src.into_bytes()
}

/// Wide record passed to a function, so every call compares the whole record type
fn synth_wide_record_param() -> Vec<u8> {
    let mut src = String::from("fn first(r: Record<");
    for i in 0..SYNTH_WIDTH {
        src.push_str(&format!("f{i}: Int "));
    }
    src.push_str(">) Int { r.f0 }\nlet r = {");
    for i in 0..SYNTH_WIDTH {
        src.push_str(&format!("f{i}: {i} "));
    }
    src.push_str("}\n");
    for i in 0..SYNTH_WIDTH {
        src.push_str(&format!("let x{i} = first(r)\n"));
    }
    src.into_bytes()
}

/// `[[[...1...]]]` plus a long homogeneous array of records
fn synth_arrays() -> Vec<u8> {
    let mut src = String::from("let deep = ");
    for _ in 0..SYNTH_DEPTH_RECORD {
        src.push('[');
    }
    src.push('1');
    for _ in 0..SYNTH_DEPTH_RECORD {
        src.push(']');
    }
    src.push_str("\nlet wide = [");
    for i in 0..SYNTH_LINES {
        src.push_str(&format!("{{x: {i} y: {i} tag: \"t\"}} "));
    }
    src.push_str("]\n");
    src.into_bytes()
}

/// Nested `match` arms, each scope a new env frame the innermost identifier walks through
fn synth_nested_match() -> Vec<u8> {
    let mut src = String::from("fn f(n:Int) Int {\n");
    for i in 0..SYNTH_DEPTH_SCOPE {
        src.push_str(&format!(
            "let v{i} = n + {i}\nmatch {{ n == {i} {{ v0 }} {{\n"
        ));
    }
    src.push_str("v0 + n");
    for _ in 0..SYNTH_DEPTH_SCOPE {
        src.push_str("\n} }");
    }
    src.push_str("\n}\nlet r = f(1)\n");
    src.into_bytes()
}

/// Overload groups with several candidates, both branches of each group
fn synth_overloads() -> Vec<u8> {
    let mut src = String::from("import (\"math\" \"str\" \"testing\")\n");
    for i in 0..SYNTH_LINES {
        src.push_str(&format!(
            "let a{i} = str.concat(str.from({i}) str.from({i}.5))\nlet b{i} = math.min(math.abs(-{i}) {i}) + math.max({i} 1)\n"
        ));
    }
    src.into_bytes()
}

fn synth_casts() -> Vec<u8> {
    let mut src = String::new();
    for i in 0..SYNTH_LINES {
        src.push_str(&format!(
            "let d{i} = ({i} as Double) * 2.0 + (1 as Double)\n"
        ));
    }
    src.into_bytes()
}

/// Many small functions with several parameters each, all called once
fn synth_many_fns() -> Vec<u8> {
    let mut src = String::new();
    for i in 0..SYNTH_FNS {
        src.push_str(&format!(
            "fn f{i}(a:Int b:Int c:Double d:Str e:Bool) Int {{ match {{ e {{ a + b }} {{ a - b }} }} }}\n"
        ));
    }
    for i in 0..SYNTH_FNS {
        src.push_str(&format!("let r{i} = f{i}({i} 1 2.0 \"s\" true)\n"));
    }
    src.into_bytes()
}

/// One function whose body is a long `let` chain, so a single env frame grows large
fn synth_long_body() -> Vec<u8> {
    let mut src = String::from("fn f(n:Int) Int {\nlet v0 = n\n");
    for i in 1..SYNTH_LINES {
        src.push_str(&format!("let v{i} = v{} + v0\n", i - 1));
    }
    src.push_str(&format!("v{}\n}}\nlet r = f(1)\n", SYNTH_LINES - 1));
    src.into_bytes()
}

fn parse<'s>(name: &str, source: &'s [u8]) -> Ast<'s, 's> {
    let arena = Box::leak(Box::new(BumpAlloc::new()));
    let parse =
        parser::Parser::new(lex::Lexer::new(source), arena, &BumpAlloc::new()).parse_collect();
    assert!(
        parse.diagnostics.is_empty(),
        "parse failed for {name}: {:?}",
        parse.diagnostics
    );
    parse
        .ast
        .expect("parser returned no diagnostics and no AST")
}

fn bench_program(c: &mut Criterion, name: &str, source: &[u8]) {
    let ast = parse(name, source);

    let (mut types, mut scratch) = (BumpAlloc::new(), BumpAlloc::new());
    let probe = Typechecker::new(&ast, &types, &scratch)
        .with_stdlib(purple_garden_std::STD)
        .check();
    assert!(
        probe.diagnostics.is_empty(),
        "typecheck failed for {name}: {:?}",
        probe.diagnostics
    );

    drop(probe);

    c.bench_function(&format!("{name}_typecheck"), |b| {
        b.iter(|| {
            types.reset();
            scratch.reset();
            Typechecker::new(&ast, &types, &scratch)
                .with_stdlib(purple_garden_std::STD)
                .check()
                .diagnostics
                .len()
        });
    });
}

pub fn typecheck(c: &mut Criterion) {
    for (name, source) in programs() {
        bench_program(c, &name, &source);
    }
    bench_program(c, "synth_std_calls", &synth_std_calls());
    bench_program(c, "synth_user_calls", &synth_user_calls());
    bench_program(c, "synth_idents", &synth_idents());
    bench_program(c, "synth_generic_calls", &synth_generic_calls());
    bench_program(c, "synth_generic_deep", &synth_generic_deep());
    bench_program(c, "synth_deep_expr", &synth_deep_expr());
    bench_program(c, "synth_deep_record", &synth_deep_record());
    bench_program(c, "synth_wide_record", &synth_wide_record());
    bench_program(c, "synth_wide_record_param", &synth_wide_record_param());
    bench_program(c, "synth_arrays", &synth_arrays());
    bench_program(c, "synth_nested_match", &synth_nested_match());
    bench_program(c, "synth_overloads", &synth_overloads());
    bench_program(c, "synth_casts", &synth_casts());
    bench_program(c, "synth_many_fns", &synth_many_fns());
    bench_program(c, "synth_long_body", &synth_long_body());
}

fn main() {
    let mut criterion = common::criterion();
    typecheck(&mut criterion);
    criterion.final_summary();
}
