use purple_garden_shared::config::Config;

use crate::{BcLineKind, Function, Inspection, inspect};

fn run(src: &str) -> Inspection {
    inspect("test.garden", src.as_bytes(), &Config::default())
}

fn func<'a>(i: &'a Inspection, name: &str) -> &'a Function {
    i.functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| {
            panic!(
                "no function {name} in {:?}",
                i.functions.iter().map(|f| &f.name).collect::<Vec<_>>()
            )
        })
}

const FIB: &str = "fn fib(n:Int) Int {
    match {
        n == 0 { 0 }
        n == 1 { 1 }
        { fib(n - 1) + fib(n - 2) }
    }
}
fib(10)
";

#[test]
fn empty_source_inspects_to_nothing() {
    let i = run("");
    assert!(i.diagnostics.is_empty());
    assert!(i.error.is_none());
    assert_eq!(
        i.phases.len(),
        7,
        "every compile phase is listed, ran or not"
    );
}

#[test]
fn parse_errors_stop_after_the_frontend() {
    let src = "fn f(a:Int) Int { a + }";
    let i = run(src);
    assert!(!i.diagnostics.is_empty());
    assert!(i.functions.is_empty());
    assert!(i.bytecode.is_empty());
    for d in &i.diagnostics {
        assert!(d.start <= src.len(), "{d:?}");
        assert!(d.rendered.starts_with("test.garden:"), "{}", d.rendered);
    }
}

#[test]
fn type_errors_stop_after_the_frontend() {
    let i = run("let x = 1 + \"a\"\nx");
    assert!(!i.diagnostics.is_empty(), "{i:#?}");
    assert!(i.functions.is_empty());
    assert!(!i.ast.is_empty(), "the ast still parsed");
    assert!(
        !i.typed_ast.is_empty(),
        "types are rendered where they could be inferred"
    );
}

#[test]
fn types_render_signatures_and_every_value() {
    let i = run(FIB);
    assert!(i.types.contains("fib: (Int) -> Int"), "{}", i.types);
    assert!(
        i.typed_ast.lines().count() > i.types.lines().count(),
        "{}",
        i.typed_ast
    );
    assert!(i.typed_ast.contains("Int"), "{}", i.typed_ast);
}

#[test]
fn self_tail_calls_are_calls() {
    let i = run(
        "fn count(n:Int acc:Int) Int { match { n == 0 { acc } { count(n - 1 acc + 1) } } }\ncount(3 0)",
    );
    let count = func(&i, "count");
    let calls: Vec<u32> = count
        .final_ir()
        .blocks
        .iter()
        .flat_map(|b| b.calls.clone())
        .collect();
    assert_eq!(calls, [count.id]);
}

#[test]
fn snapshots_follow_the_pass_pipeline() {
    let i = run(FIB);
    let fib = func(&i, "fib");
    let passes: Vec<&str> = fib.snapshots.iter().map(|s| s.pass.as_str()).collect();
    let mut want = vec!["lower"];
    want.extend(purple_garden_opt::Pass::PIPELINE.iter().map(|p| p.name()));
    want.push("dce_functions");
    assert_eq!(passes, want);
    assert!(
        fib.snapshots[0].ir.is_some(),
        "the lowered IR is always recorded"
    );
    assert!(
        fib.snapshots[1..].iter().any(|s| s.ir.is_none()),
        "some pass leaves fib alone"
    );
    assert!(
        fib.snapshots[1..].iter().any(|s| s.ir.is_some()),
        "some pass changes fib"
    );
    // an unchanged snapshot resolves to the last changed one
    for idx in 1..fib.snapshots.len() {
        if fib.snapshots[idx].ir.is_none() {
            assert_eq!(fib.ir_at(idx), fib.ir_at(idx - 1));
        }
    }
    // a recorded snapshot really differs from the one before it
    for idx in 1..fib.snapshots.len() {
        if let Some(ir) = &fib.snapshots[idx].ir {
            assert_ne!(ir, fib.ir_at(idx - 1), "pass {}", fib.snapshots[idx].pass);
        }
    }
    assert_eq!(fib.ir_at(usize::MAX), fib.final_ir());
}

#[test]
fn without_opt_only_the_lowered_ir_is_recorded() {
    let config = Config {
        opt: 0,
        ..Config::default()
    };
    let i = inspect("test.garden", FIB.as_bytes(), &config);
    for f in &i.functions {
        assert_eq!(f.snapshots.len(), 1, "{}", f.name);
    }
    assert!(!i.bytecode.is_empty());
}

#[test]
fn blocks_cover_their_rows_and_edges_stay_inside_the_function() {
    let i = run(FIB);
    for f in &i.functions {
        for s in f.snapshots.iter().filter_map(|s| s.ir.as_ref()) {
            let ids: Vec<u32> = s.blocks.iter().map(|b| b.id).collect();
            for b in &s.blocks {
                let (first, last) = b.rows;
                assert!(first <= last && last < s.rows.len());
                assert!(s.rows[first].text.starts_with(&format!("b{}(", b.id)));
                assert!(s.rows[first..=last].iter().all(|r| r.block == Some(b.id)));
                for succ in &b.succs {
                    assert!(ids.contains(succ), "b{} -> b{succ} in {}", b.id, f.name);
                }
            }
        }
    }
    let fib = func(&i, "fib");
    assert!(
        fib.final_ir().blocks.iter().any(|b| b.succs.len() == 2),
        "fib branches"
    );
    let calls: Vec<u32> = fib
        .final_ir()
        .blocks
        .iter()
        .flat_map(|b| b.calls.clone())
        .collect();
    assert_eq!(calls, [fib.id, fib.id], "fib calls itself twice");
    let entry = func(&i, "entry");
    assert!(
        entry
            .final_ir()
            .blocks
            .iter()
            .any(|b| b.calls.contains(&fib.id)),
        "the entry calls fib"
    );
}

#[test]
fn bytecode_lines_point_back_to_ir_rows_and_source() {
    let i = run(FIB);
    let fib = func(&i, "fib");
    let rows: Vec<u32> = fib.final_ir().rows.iter().filter_map(|r| r.pos).collect();
    let max_row = *rows.iter().max().unwrap();
    let fib_ops: Vec<_> = i
        .bytecode
        .iter()
        .filter(|l| l.kind == BcLineKind::Op && l.func == Some(fib.id))
        .collect();
    assert!(!fib_ops.is_empty());
    for op in &fib_ops {
        assert!(op.row.unwrap() <= max_row, "{op:?}");
        assert!((op.span.unwrap() as usize) < FIB.len(), "{op:?}");
    }
    assert!(
        i.bytecode
            .iter()
            .any(|l| l.kind == BcLineKind::Func && l.text == "fib")
    );
    // every op before the first function label belongs to none, every op after to one
    let first_func = i
        .bytecode
        .iter()
        .position(|l| l.kind == BcLineKind::Func)
        .unwrap();
    assert!(i.bytecode[first_func..].iter().all(|l| l.func.is_some()));
}

#[test]
fn register_allocation_is_recorded_for_bytecode_and_x86() {
    let i = run(FIB);
    let fib = func(&i, "fib");
    assert_eq!(fib.bc_locations.len(), fib.intervals.len());
    for (value, interval) in fib.intervals.iter().enumerate() {
        if interval.is_some() {
            assert!(
                fib.bc_locations[value].is_some(),
                "%v{value} is live but has no bytecode location"
            );
        }
    }
    let native = fib.x86.as_ref().expect("fib compiles to x86-64");
    assert_eq!(native.locations.len(), fib.intervals.len());
    assert!(
        native.locations.iter().flatten().any(
            |l| matches!(l, crate::Location::Reg(r) if r.starts_with('r') || r.starts_with('e'))
        ),
        "{:?}",
        native.locations
    );
}

#[test]
fn x86_is_disassembled_with_labels_and_rows() {
    let i = run(FIB);
    let fib = func(&i, "fib");
    let native = fib.x86.as_ref().expect("fib compiles to x86-64");
    assert!(native.size > 0);
    assert_eq!(native.lines[0].offset, 0);
    assert_eq!(
        native.lines[0].label.as_deref(),
        Some(format!("f{} <fib>", fib.id).as_str())
    );
    assert!(native.lines.iter().any(|l| l.text.starts_with("ret")));
    assert!(
        native
            .lines
            .iter()
            .any(|l| l.text.starts_with("call") && l.text.contains("<fib>")),
        "the recursive call resolves to fib: {:#?}",
        native.lines
    );
    assert!(
        native
            .lines
            .iter()
            .any(|l| l.label.as_deref().is_some_and(|l| l.starts_with('b')))
    );
    // offsets ascend and the bytes add up to the function
    let mut end = 0;
    for line in &native.lines {
        assert_eq!(line.offset, end, "{line:?}");
        end += line.bytes.split(' ').count();
    }
    assert_eq!(end, native.size);
    let max_row = fib.intervals.len() as u32 * 2 + 64;
    assert!(
        native
            .lines
            .iter()
            .filter_map(|l| l.row)
            .all(|r| r < max_row)
    );
    assert!(native.lines.iter().any(|l| l.row.is_some()));
}

#[test]
fn natives_follow_each_other_in_the_arena() {
    // recursive, so neither is inlined into its caller
    let i = run(&format!(
        "{}\nfn twice(a:Int) Int {{ match {{ a == 0 {{ 0 }} {{ fib(a) + twice(a - 1) }} }} }}\ntwice(3)",
        FIB.lines().take(7).collect::<Vec<_>>().join("\n")
    ));
    let inc = func(&i, "fib").x86.as_ref().unwrap();
    let twice = func(&i, "twice").x86.as_ref().unwrap();
    assert!(twice.address >= inc.address + inc.size as u64);
    assert_eq!(twice.address % 16, 0);
}

#[test]
fn unused_functions_are_marked_and_not_compiled() {
    let i = run(&format!(
        "fn unused(a:Int) Int {{ a * 3 }}\n{}\nfib(2)",
        FIB.lines().take(7).collect::<Vec<_>>().join("\n")
    ));
    let unused = func(&i, "unused");
    assert!(unused.unused);
    assert!(unused.x86.is_err());
    assert!(unused.bc_locations.is_empty());
    assert!(!func(&i, "fib").unused);
}

#[test]
fn unsupported_functions_say_why() {
    // string concatenation calls into the runtime with a string the jit can't hold in a register
    let i = run("import \"str\"\nfn f(a:Str) Str { str.concat(a \"!\") }\nf(\"x\")");
    for f in &i.functions {
        if let Err(reason) = &f.x86 {
            assert!(!reason.is_empty(), "{}", f.name);
        }
    }
}

#[test]
fn phases_account_for_time_and_memory() {
    let i = run(FIB);
    let names: Vec<_> = i.phases.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "scratch",
            "parse",
            "typecheck",
            "lower ir",
            "opt",
            "bytecode",
            "jit"
        ]
    );
    let parse = &i.phases[1];
    assert!(parse.allocs > 0 && parse.bytes > 0 && parse.peak > 0);
}

#[test]
fn inspections_round_trip_through_json() {
    let i = run(FIB);
    let json = serde_json::to_string(&i).unwrap();
    let back: Inspection = serde_json::from_str(&json).unwrap();
    assert_eq!(back, i);
}

#[test]
fn every_example_inspects() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples");
    let mut seen = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "garden") {
            continue;
        }
        let src = std::fs::read(&path).unwrap();
        let i = inspect("test.garden", &src, &Config::default());
        assert!(
            i.diagnostics.is_empty(),
            "{}: {:#?}",
            path.display(),
            i.diagnostics
        );
        assert!(i.error.is_none(), "{}: {:?}", path.display(), i.error);
        assert!(!i.functions.is_empty(), "{}", path.display());
        assert!(!i.bytecode.is_empty(), "{}", path.display());
        serde_json::to_string(&i).unwrap();
        seen += 1;
    }
    assert!(seen > 10);
}
