use purple_garden::config::Config;

fn run_source(input: &[u8]) {
    let config = Config::default();
    let mut program = purple_garden::new(&config, input).expect("compilation failed");
    program.run().expect("program run failed");
}

fn run_source_opt(input: &[u8]) {
    let mut config = Config::default();
    config.opt = 3;
    let mut program = purple_garden::new(&config, input).expect("compilation failed");
    program.run().expect("program run failed");
}

include!(concat!(env!("OUT_DIR"), "/example_tests.rs"));
