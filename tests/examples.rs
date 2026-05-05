use purple_garden::config::Config;

fn run_source(input: &[u8]) {
    let config = Config::default();
    let mut vm = purple_garden::new(&config, input).expect("compilation failed");
    vm.run().expect("vm run failed");
}

fn run_source_opt(input: &[u8]) {
    let mut config = Config::default();
    config.opt = 3;
    let mut vm = purple_garden::new(&config, input).expect("compilation failed");
    vm.run().expect("vm run failed");
}

include!(concat!(env!("OUT_DIR"), "/example_tests.rs"));
