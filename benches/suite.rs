//! End-to-end workload suite.
//!
//! Walks `benches/programs/*.garden` and generates four criterion benches
//! per program — `<name>_compile`, `<name>_compile_opt`, `<name>_run`,
//! `<name>_run_opt` — so compile cost and run cost are separated cleanly.
//!
//! Each program self-asserts its result via `testing.assert`. Before the
//! timed iteration loop we run the VM once and `expect()` success, so a
//! codegen bug shows up as a panic at bench startup rather than as
//! silently-faster numbers.

use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main};
use purple_garden::config::Config;

fn programs() -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/programs");
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
        .expect("benches/programs dir missing")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("garden"))
        .map(|p| {
            let name = p.file_stem().unwrap().to_string_lossy().into_owned();
            let bytes = std::fs::read(&p)
                .unwrap_or_else(|e| panic!("read {}: {}", p.display(), e));
            (name, bytes)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

const CFG: &Config = &Config::default();
const CFG_OPT: &Config = &{
    let mut c = Config::default();
    c.opt = 1;
    c
};

pub fn suite(c: &mut Criterion) {
    for (name, source) in programs() {
        // Validate once at startup so codegen bugs panic loudly here, not
        // silently affect the timing.
        let (mut probe, _debug) = purple_garden::new(CFG_OPT, &source)
            .unwrap_or_else(|e| panic!("compile failed for {name}: {e:?}"));
        probe
            .run()
            .unwrap_or_else(|e| panic!("run failed for {name}: {e:?}"));

        c.bench_function(&format!("{name}_compile"), |b| {
            b.iter(|| {
                purple_garden::new(CFG, &source).unwrap();
            })
        });
        c.bench_function(&format!("{name}_compile_opt"), |b| {
            b.iter(|| {
                purple_garden::new(CFG_OPT, &source).unwrap();
            })
        });
        c.bench_function(&format!("{name}_run"), |b| {
            let (mut vm, _debug) = purple_garden::new(CFG, &source).unwrap();
            let entry = vm.pc;
            b.iter(|| {
                vm.pc = entry;
                vm.run()
            })
        });
        c.bench_function(&format!("{name}_run_opt"), |b| {
            let (mut vm, _debug) = purple_garden::new(CFG_OPT, &source).unwrap();
            let entry = vm.pc;
            b.iter(|| {
                vm.pc = entry;
                vm.run()
            })
        });
    }
}

criterion_group!(suite_group, suite);
criterion_main!(suite_group);
