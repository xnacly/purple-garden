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
/// seconds. hyperfine 1.x puts the statistics on the entry itself, with
/// `stddev` null for a single run, and reports the `-n` label as `command`;
/// 2.x nests them under `summary`, one [`Stats`] per metric, and keeps the
/// label in `name`.
#[derive(Deserialize)]
struct Export {
    results: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    /// The command line; 1.x replaces it with the `-n` label.
    command: String,
    /// The `-n` label, i.e. the runtime name, in 2.x.
    name: Option<String>,
    mean: Option<f64>,
    stddev: Option<f64>,
    min: Option<f64>,
    user: Option<f64>,
    system: Option<f64>,
    summary: Option<Summary>,
}

#[derive(Deserialize)]
struct Summary {
    time_wall_clock: Stats,
    time_user: Stats,
    time_system: Stats,
}

#[derive(Deserialize)]
struct Stats {
    mean: f64,
    stddev: Option<f64>,
    min: f64,
}

impl Entry {
    /// `(wall, user, system)` in seconds, whichever schema the export used.
    fn times(&self) -> Result<(Stats, f64, f64), String> {
        if let Some(s) = &self.summary {
            return Ok((
                Stats {
                    mean: s.time_wall_clock.mean,
                    stddev: s.time_wall_clock.stddev,
                    min: s.time_wall_clock.min,
                },
                s.time_user.mean,
                s.time_system.mean,
            ));
        }
        match (self.mean, self.min, self.user, self.system) {
            (Some(mean), Some(min), Some(user), Some(system)) => Ok((
                Stats {
                    mean,
                    stddev: self.stddev,
                    min,
                },
                user,
                system,
            )),
            _ => Err(format!(
                "hyperfine json: `{}` has neither 1.x fields nor a 2.x `summary`",
                self.command
            )),
        }
    }
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
    export
        .results
        .into_iter()
        .map(|r| {
            let (wall, user, system) = r.times()?;
            Ok(Measurement {
                workload: workload.to_string(),
                runtime: r.name.unwrap_or(r.command),
                time_ms: TimeStats {
                    mean: wall.mean * 1e3,
                    min: wall.min * 1e3,
                    stddev: wall.stddev.unwrap_or(0.0) * 1e3,
                },
                cpu_ms: (user + system) * 1e3,
                memory_mb: None,
            })
        })
        .collect()
}
