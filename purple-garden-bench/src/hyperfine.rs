//! Driving hyperfine and reading its JSON export.

use std::{
    fs,
    process::{Command, Stdio},
};

use serde::Deserialize;

use crate::{
    report::{Measurement, TimeStats},
    runtime::Runtime,
    workload::Workload,
};

/// The slice of hyperfine's `--export-json` schema we read. Times are
/// seconds; `stddev` is null for a single run.
#[derive(Deserialize)]
struct Export {
    results: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    /// The `-n` label, i.e. the runtime name.
    command: String,
    mean: f64,
    stddev: Option<f64>,
    min: f64,
    user: f64,
    system: f64,
}

/// Benchmark every runtime on one workload in a single hyperfine invocation
/// (`-N`: no shell), each command labelled with its runtime name.
pub fn measure(
    w: &Workload,
    runtimes: &[Runtime],
    runs: u32,
    warmup: u32,
) -> Result<Vec<Measurement>, String> {
    let export = std::env::temp_dir().join(format!(
        "pg-bench-{}-{}.json",
        w.name,
        std::process::id()
    ));
    let mut cmd = Command::new("hyperfine");
    cmd.args([
        "-N",
        "--style",
        "none",
        "--warmup",
        &warmup.to_string(),
        "--runs",
        &runs.to_string(),
        "--export-json",
    ])
    .arg(&export);
    for rt in runtimes {
        let parts = w.command(rt);
        if parts.iter().any(|p| p.chars().any(char::is_whitespace)) {
            return Err(format!(
                "paths with whitespace are not supported with hyperfine -N: {parts:?}"
            ));
        }
        cmd.args(["-n", rt.name()]).arg(parts.join(" "));
    }
    let status = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("hyperfine: {e} (is it installed?)"))?;
    if !status.success() {
        return Err(format!("hyperfine failed on workload `{}`", w.name));
    }
    let text =
        fs::read_to_string(&export).map_err(|e| format!("read {}: {e}", export.display()))?;
    let _ = fs::remove_file(&export);
    parse(&w.name, &text)
}

/// Convert hyperfine's seconds into the report's milliseconds. Memory is
/// filled in separately by `rusage`.
pub fn parse(workload: &str, json: &str) -> Result<Vec<Measurement>, String> {
    let export: Export =
        serde_json::from_str(json).map_err(|e| format!("hyperfine json: {e}"))?;
    Ok(export
        .results
        .into_iter()
        .map(|r| Measurement {
            workload: workload.to_string(),
            runtime: r.command,
            time_ms: TimeStats {
                mean: r.mean * 1e3,
                min: r.min * 1e3,
                stddev: r.stddev.unwrap_or(0.0) * 1e3,
            },
            cpu_ms: (r.user + r.system) * 1e3,
            memory_mb: None,
        })
        .collect())
}
