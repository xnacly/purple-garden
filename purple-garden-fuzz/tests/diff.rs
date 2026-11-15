//! The libFuzzer `diff` target without libFuzzer: generated programs from a fixed range of seeds
//! on every backend. `PG_DIFF_SEEDS` (default 200) and `PG_DIFF_START` (default 0) pick the
//! range, so a longer run is `PG_DIFF_SEEDS=100000 cargo test -p purple-garden-fuzz --release`.
//! The JIT only compiles on x86-64, run with `--target x86_64-apple-darwin` on Apple silicon.

use purple_garden_fuzz::{
    oracle::{self, Outcome},
    seeded,
};

const BYTES: usize = 2048;

fn env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[test]
fn seeded_programs_agree_on_every_backend() {
    let start = env("PG_DIFF_START", 0);
    let seeds = env("PG_DIFF_SEEDS", 200);
    let mut failures = Vec::new();
    let mut values = 0;
    for seed in start..start + seeds {
        let src = seeded(seed, BYTES);
        match oracle::check(&src) {
            Ok(Outcome::Rejected(diagnostic)) => failures.push(format!(
                "seed {seed}: the generator produced a program the compiler rejects: {diagnostic}"
            )),
            Ok(Outcome::Value(_)) => values += 1,
            Ok(Outcome::Anomaly(_)) => {}
            Err(divergence) => failures.push(format!("seed {seed}: {divergence}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {seeds} seeds failed, `cargo run -p purple-garden-fuzz --bin pg-diff -- show <seed>` \
         prints a program, `reduce` minimises it:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // most programs have to get to their checksum, or anomalies hide everything after them
    assert!(
        values * 2 > seeds,
        "only {values} of {seeds} programs ran to a value"
    );
}

#[test]
fn a_seed_always_generates_the_same_program() {
    for seed in 0..20 {
        assert_eq!(seeded(seed, BYTES), seeded(seed, BYTES));
    }
}

#[test]
fn exhausted_input_generates_a_minimal_program() {
    let src = purple_garden_fuzz::generate::program(&mut arbitrary::Unstructured::new(&[]))
        .expect("generation is total");
    assert_eq!(oracle::check(&src).expect("agrees"), Outcome::Value(0));
}
