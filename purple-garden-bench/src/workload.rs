//! Workload directories: one program in four languages per directory.

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
/// rather than hyperfine's generic non-zero-exit abort.
pub fn preflight(workloads: &[Workload], runtimes: &[Runtime]) -> Result<(), String> {
    let mut failures = Vec::new();
    for w in workloads {
        for rt in runtimes {
            let cmd = w.command(rt);
            let tag = format!("{}/{}", w.name, rt.name());
            if !Path::new(&cmd[1]).is_file() {
                failures.push(format!("{tag}: missing {}", cmd[1]));
                continue;
            }
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
            "preflight failed (wrong result or missing port):\n  {}",
            failures.join("\n  ")
        ))
    }
}
