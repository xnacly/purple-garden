fn main() {
    // raylib is supplied by the host system.  Keeping this link in the vendor
    // crate means the core interpreter and std crate remain raylib-independent.
    println!("cargo:rerun-if-env-changed=RAYLIB_LIB_DIR");
    if let Ok(dir) = std::env::var("RAYLIB_LIB_DIR") {
        println!("cargo:rustc-link-search=native={dir}");
    }
    let mut probe = std::process::Command::new("pkg-config");
    probe.args(["--libs", "raylib"]);
    if let Ok(output) = probe.output().and_then(|output| {
        if output.status.success() {
            Ok(output)
        } else {
            Err(std::io::Error::other("pkg-config failed"))
        }
    }) {
        for flag in String::from_utf8_lossy(&output.stdout).split_whitespace() {
            if let Some(path) = flag.strip_prefix("-L") {
                println!("cargo:rustc-link-search=native={path}");
            } else if let Some(lib) = flag.strip_prefix("-l") {
                println!("cargo:rustc-link-lib={lib}");
            }
        }
    } else {
        println!("cargo:rustc-link-lib=raylib");
    }
}
