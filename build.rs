use std::path::PathBuf;
use std::process::Command;

fn cmd(cmd: &str, args: &[&str]) -> String {
    String::from_utf8(
        Command::new(cmd)
            .args(args)
            .output()
            .unwrap_or_else(|_| panic!("{cmd} failed"))
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string()
}

fn main() {
    println!(
        "cargo:rustc-env=GIT_HASH={}",
        cmd("git", &["rev-parse", "--short", "HEAD"])
    );

    println!(
        "cargo:rustc-env=GIT_SUBJECT={}",
        cmd("git", &["log", "-1", "--pretty=%s"])
    );

    println!(
        "cargo:rustc-env=GIT_DESCRIBE={}",
        cmd("git", &["log", "-1", "--pretty=%h %s"])
    );

    println!(
        "cargo:rustc-env=BUILD_TIMESTAMP={}",
        cmd("date", &["-u", "+%Y-%m-%d"])
    );

    let mut features = Vec::new();
    for (key, _) in std::env::vars() {
        if let Some(name) = key.strip_prefix("CARGO_FEATURE_") {
            features.push(name.to_lowercase());
        }
    }
    features.sort();
    println!("cargo:rustc-env=BUILD_FEATURES={}", features.join(","));

    println!(
        "cargo:rustc-env=BUILD_PROFILE={}",
        std::env::var("PROFILE").unwrap()
    );

    generate_example_tests();
}

fn generate_example_tests() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let examples_dir = PathBuf::from(&manifest_dir).join("examples");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = PathBuf::from(&out_dir).join("example_tests.rs");

    println!("cargo:rerun-if-changed=examples");

    let mut entries: Vec<_> = std::fs::read_dir(&examples_dir)
        .unwrap_or_else(|e| panic!("read examples/: {e}"))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("garden"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut out = String::new();
    for entry in entries {
        let path = entry.path();
        let stem = path.file_stem().unwrap().to_string_lossy();
        let fn_name: String = stem
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let abs = path.to_string_lossy();
        out.push_str(&format!(
            "#[test]\nfn {fn_name}() {{\n    run_source(include_bytes!(r\"{abs}\"));\n}}\n\n\
             #[test]\nfn {fn_name}_opt() {{\n    run_source_opt(include_bytes!(r\"{abs}\"));\n}}\n\n"
        ));
    }

    std::fs::write(&out_path, out).unwrap_or_else(|e| panic!("write {out_path:?}: {e}"));
}
