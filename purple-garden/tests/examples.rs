use purple_garden_shared::config::Config;
use std::path::PathBuf;

fn examples() -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
        .expect("examples dir missing")
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("garden"))
        .map(|path| {
            let name = path
                .file_stem()
                .expect("example path has no file stem")
                .to_string_lossy()
                .into_owned();
            let source = std::fs::read(&path)
                .unwrap_or_else(|err| panic!("failed to read {}: {}", path.display(), err));
            (name, source)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn run_source(name: &str, input: &[u8]) {
    let config = Config::default();
    let mut program = purple_garden::Pg::new()
        .with_stdlib()
        .with_unsafe_stdlib()
        .config(config)
        .compile(input)
        .unwrap_or_else(|err| panic!("compilation failed for {name}: {err:?}"));
    program
        .run()
        .unwrap_or_else(|err| panic!("program run failed for {name}: {err:?}"));
}

fn run_source_opt(name: &str, input: &[u8]) {
    let mut config = Config::default();
    config.opt = 3;
    let mut program = purple_garden::Pg::new()
        .with_stdlib()
        .with_unsafe_stdlib()
        .config(config)
        .compile(input)
        .unwrap_or_else(|err| panic!("optimized compilation failed for {name}: {err:?}"));
    program
        .run()
        .unwrap_or_else(|err| panic!("optimized program run failed for {name}: {err:?}"));
}

#[test]
fn garden_examples() {
    for (name, source) in examples() {
        run_source(&name, &source);
    }
}

#[test]
fn optimized_garden_examples() {
    for (name, source) in examples() {
        run_source_opt(&name, &source);
    }
}
