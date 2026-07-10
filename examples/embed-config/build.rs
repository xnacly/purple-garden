#[path = "src/config.rs"]
mod config_pkg;

use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=src/config.rs");
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let extern_path = manifest_dir.join("extern.garden");
    fs::write(&extern_path, config_pkg::config::PACKAGE.extern_source())
        .expect("failed to write extern.garden");
}
