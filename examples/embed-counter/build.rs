#[path = "src/counter.rs"]
mod counter_pkg;

use std::{env, fs, path::PathBuf};

fn main() {
    // Write the generated package signatures next to the example crate so
    // tooling can load them without asking the binary to print anything.
    println!("cargo:rerun-if-changed=src/counter.rs");
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let extern_path = manifest_dir.join("extern.garden");
    fs::write(&extern_path, counter_pkg::counter::PACKAGE.extern_source())
        .expect("failed to write extern.garden");
}
