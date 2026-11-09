//! The runtimes under test: how to invoke each one and read its version.

use std::{path::Path, process::Command};

/// Static description of one runtime. `bin == None` means the purple-garden
/// binary given on the command line.
pub struct Spec {
    pub name: &'static str,
    bin: Option<&'static str>,
    /// Source file name inside a workload directory.
    pub source: &'static str,
    /// Flags passed before the source file.
    pub args: &'static [&'static str],
    /// Environment set for every run, JavaScriptCore only takes its JIT switch this way.
    pub env: &'static [(&'static str, &'static str)],
    version_args: &'static [&'static str],
}

/// Every JIT runtime also runs as `<name>-nojit` with its JIT disabled. python3 is
/// interpreter-only unless built with `--enable-experimental-jit`, which is off by default.
pub const SPECS: &[Spec] = &[
    Spec {
        name: "garden",
        bin: None,
        source: "main.garden",
        args: &[],
        env: &[],
        version_args: &["-V"],
    },
    Spec {
        name: "garden-nojit",
        bin: None,
        source: "main.garden",
        args: &["--no-jit"],
        env: &[],
        version_args: &["-V"],
    },
    Spec {
        name: "bun",
        bin: Some("bun"),
        source: "main.js",
        args: &[],
        env: &[],
        version_args: &["--version"],
    },
    Spec {
        name: "bun-nojit",
        bin: Some("bun"),
        source: "main.js",
        args: &[],
        env: &[("BUN_JSC_useJIT", "0")],
        version_args: &["--version"],
    },
    Spec {
        name: "luajit",
        bin: Some("luajit"),
        source: "main.lua",
        args: &[],
        env: &[],
        version_args: &["-v"],
    },
    Spec {
        name: "luajit-nojit",
        bin: Some("luajit"),
        source: "main.lua",
        args: &["-joff"],
        env: &[],
        version_args: &["-v"],
    },
    Spec {
        name: "python3",
        bin: Some("python3"),
        source: "main.py",
        args: &[],
        env: &[],
        version_args: &["--version"],
    },
];

/// A runtime that resolved on this machine.
pub struct Runtime {
    pub spec: &'static Spec,
    pub bin: String,
    pub version: String,
}

impl Runtime {
    pub fn name(&self) -> &'static str {
        self.spec.name
    }
}

/// Probe every runtime by asking for its version. A missing binary is
/// skipped with a warning, or is an error with `strict`.
pub fn resolve(garden: &Path, strict: bool) -> Result<Vec<Runtime>, String> {
    let mut out = Vec::new();
    for spec in SPECS {
        let bin = spec
            .bin
            .map(str::to_string)
            .unwrap_or_else(|| garden.to_string_lossy().into_owned());
        match Command::new(&bin).args(spec.version_args).output() {
            Ok(o) => out.push(Runtime {
                spec,
                version: first_line(&o.stdout, &o.stderr),
                bin,
            }),
            Err(e) if strict => {
                return Err(format!("runtime `{}` ({bin}) unavailable: {e}", spec.name));
            }
            Err(e) => eprintln!("skipping `{}`: {bin}: {e}", spec.name),
        }
    }
    if out.is_empty() {
        return Err("no runtimes available".into());
    }
    Ok(out)
}

/// First line of stdout, or of stderr when stdout is empty (`luajit -v` and
/// some `--version` flags print there).
fn first_line(stdout: &[u8], stderr: &[u8]) -> String {
    let text = if stdout.is_empty() { stderr } else { stdout };
    String::from_utf8_lossy(text)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}
