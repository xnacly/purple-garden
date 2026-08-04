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

    println!(
        "cargo:rustc-env=BUILD_TARGET={}",
        std::env::var("TARGET").unwrap()
    );

    println!(
        "cargo:rustc-env=BUILD_OPT_LEVEL={}",
        std::env::var("OPT_LEVEL").unwrap()
    );

    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let version = cmd(&rustc, &["-vV"]);
    println!(
        "cargo:rustc-env=BUILD_RUSTC={}",
        version.lines().next().unwrap_or_default()
    );

    // Without this the flags are frozen at whatever the first build saw.
    println!("cargo:rerun-if-env-changed=RUSTFLAGS");
    println!("cargo:rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
    println!(
        "cargo:rustc-env=BUILD_RUSTFLAGS={}",
        std::env::var("CARGO_ENCODED_RUSTFLAGS")
            .unwrap_or_default()
            .replace('\x1f', " ")
    );
}
