//! Workload directories: one program per directory, ported to up to four
//! languages. A runtime without a port is skipped for that workload.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::runtime::Runtime;

pub struct Workload {
    pub name: String,
    pub dir: PathBuf,
}

impl Workload {
    /// Whether this workload has a port for `rt`.
    pub fn has(&self, rt: &Runtime) -> bool {
        self.dir.join(rt.spec.source).is_file()
    }

    /// The runtimes this workload is ported to, in `runtimes` order.
    pub fn ported<'r>(&self, runtimes: &'r [Runtime]) -> Vec<&'r Runtime> {
        runtimes.iter().filter(|rt| self.has(rt)).collect()
    }

    /// `[program, source_file]` to run this workload on `rt`.
    pub fn command(&self, rt: &Runtime) -> Vec<String> {
        vec![
            rt.bin.clone(),
            self.dir.join(rt.spec.source).to_string_lossy().into_owned(),
        ]
    }
}

/// Every sub-directory of `dir` holding a `main.garden`, sorted by name and
/// optionally narrowed to `only`.
pub fn discover(dir: &Path, only: Option<&str>) -> Result<Vec<Workload>, String> {
    let mut out: Vec<Workload> = fs::read_dir(dir)
        .map_err(|e| format!("workloads dir {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("main.garden").is_file())
        .map(|p| Workload {
            name: p.file_name().unwrap().to_string_lossy().into_owned(),
            dir: p,
        })
        .filter(|w| only.is_none_or(|o| o == w.name))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    if out.is_empty() {
        return Err(match only {
            Some(o) => format!("no workload named `{o}` under {}", dir.display()),
            None => format!("no workloads under {}", dir.display()),
        });
    }
    Ok(out)
}

/// Run every program once so a wrong result fails with a readable message
/// rather than hyperfine's generic non-zero-exit abort. A missing port is
/// noted and skipped: some workloads are left out for a runtime on purpose,
/// e.g. where CPython would take minutes.
pub fn preflight(workloads: &[Workload], runtimes: &[Runtime]) -> Result<(), String> {
    let mut failures = Vec::new();
    for w in workloads {
        for rt in runtimes {
            let tag = format!("{}/{}", w.name, rt.name());
            if !w.has(rt) {
                eprintln!("{tag}: no {} port, skipped", rt.spec.source);
                continue;
            }
            let cmd = w.command(rt);
            let status = Command::new(&cmd[0])
                .args(&cmd[1..])
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .status();
            match status {
                Ok(s) if s.success() => {}
                Ok(s) => failures.push(format!("{tag}: exit {}", s.code().unwrap_or(-1))),
                Err(e) => failures.push(format!("{tag}: {e}")),
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "preflight failed (wrong result):\n  {}",
            failures.join("\n  ")
        ))
    }
}
