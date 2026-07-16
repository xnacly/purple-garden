use purple_garden_shared::config::Config;
use std::process::Command;

fn run_source(input: &[u8]) {
    let config = Config::default();
    let mut program = purple_garden::Pg::new()
        .with_stdlib()
        .with_unsafe_stdlib()
        .config(config)
        .compile(input)
        .expect("compilation failed");
    program.run().expect("program run failed");
}

fn run_source_opt(input: &[u8]) {
    let mut config = Config::default();
    config.opt = 3;
    let mut program = purple_garden::Pg::new()
        .with_stdlib()
        .with_unsafe_stdlib()
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

#[test]
fn embed_config_example() {
    let manifest_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/embed-config");
    let output = Command::new(env!("CARGO"))
        .args(["run", "--quiet"])
        .current_dir(manifest_dir)
        .output()
        .expect("failed to run embed-config example");

    assert!(
        output.status.success(),
        "embed-config failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("api: workers=4 mode=debug retry=3x/250ms"),
        "embed-config output did not include the config summary\nstdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

macro_rules! example_tests {
    ($($name:ident => $path:literal,)*) => {
        $(
            #[test]
            fn $name() {
                run_source(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../", $path)));
            }

            mod $name {
                use super::*;

                #[test]
                fn opt() {
                    run_source_opt(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../", $path)));
                }
            }
        )*
    };
}

example_tests! {
    ackermann => "examples/ackermann.garden",
    call_chain => "examples/call_chain.garden",
    collatz => "examples/collatz.garden",
    factorial => "examples/factorial.garden",
    fib => "examples/fib.garden",
    functions => "examples/functions.garden",
    jitprogress => "examples/jitprogress.garden",
    math => "examples/math.garden",
    mandelbrot => "examples/mandelbrot.garden",
    many_functions => "examples/many_functions.garden",
    regressions => "examples/regressions.garden",
    tak => "examples/tak.garden",
    wide_match => "examples/wide_match.garden",
}
