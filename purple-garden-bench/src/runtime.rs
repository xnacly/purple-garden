//! The runtimes under test: how to invoke each one and read its version.

use std::{path::Path, process::Command};

/// Static description of one runtime. `bin == None` means the purple-garden
/// binary given on the command line.
pub struct Spec {
    pub name: &'static str,
    bin: Option<&'static str>,
    /// Source file name inside a workload directory.
    pub source: &'static str,
    version_args: &'static [&'static str],
}

pub const SPECS: &[Spec] = &[
    Spec {
        name: "garden",
        bin: None,
        source: "main.garden",
        version_args: &["-V"],
    },
    Spec {
        name: "bun",
        bin: Some("bun"),
        source: "main.js",
        version_args: &["--version"],
    },
    Spec {
        name: "luajit",
        bin: Some("luajit"),
        source: "main.lua",
        version_args: &["-v"],
    },
    Spec {
        name: "python3",
        bin: Some("python3"),
        source: "main.py",
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
