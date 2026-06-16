use purple_garden_shared::config::Config;
use std::process::Command;

fn run_source(input: &[u8]) {
    let config = Config::default();
    let mut program = purple_garden::Pg::new()
        .config(config)
        .compile(input)
        .expect("compilation failed");
    program.run().expect("program run failed");
}

fn run_source_opt(input: &[u8]) {
    let mut config = Config::default();
    config.opt = 3;
    let mut program = purple_garden::Pg::new()
        .config(config)
        .compile(input)
        .expect("compilation failed");
    program.run().expect("program run failed");
}

#[test]
fn embed_counter_example() {
    let manifest_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/embed-counter");
    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet"])
        .current_dir(manifest_dir)
        .output()
        .expect("failed to run embed-counter example");

    assert!(
        output.status.success(),
        "embed-counter failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Counter"),
        "embed-counter output did not include the returned counter\nstdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

include!(concat!(env!("OUT_DIR"), "/example_tests.rs"));
