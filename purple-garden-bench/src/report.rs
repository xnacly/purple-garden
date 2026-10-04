//! The JSON report written per run, its markdown rendering, and the
//! `index.json` the published site reads.

use std::{
    collections::BTreeMap,
    fs,
    io::Write as _,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{RunArgs, runtime::Runtime};

#[derive(Serialize, Deserialize)]
pub struct Report {
    pub sha: String,
    /// Commit date (ISO-8601), so a history plots in commit order.
    pub date: String,
    pub run_unix: u64,
    /// Branch or tag the run was made from (`GITHUB_REF_NAME` in CI).
    #[serde(default)]
    pub r#ref: String,
    pub runner: String,
    pub runs: u32,
    pub warmup: u32,
    pub runtimes: BTreeMap<String, String>,
    pub results: Vec<Measurement>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Measurement {
    pub workload: String,
    pub runtime: String,
    pub time_ms: TimeStats,
    pub cpu_ms: f64,
    pub memory_mb: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TimeStats {
    pub mean: f64,
    pub min: f64,
    pub stddev: f64,
}

/// One row of `index.json`: enough to order and fetch a report.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct IndexEntry {
    pub file: String,
    pub sha: String,
    pub date: String,
    pub run_unix: u64,
    #[serde(default)]
    pub r#ref: String,
}

impl Report {
    pub fn new(args: &RunArgs, runtimes: &[Runtime], results: Vec<Measurement>) -> Self {
        Self {
            sha: git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".into()),
            date: git(&["log", "-1", "--format=%cI"]).unwrap_or_else(|| "unknown".into()),
            run_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            r#ref: std::env::var("GITHUB_REF_NAME")
                .ok()
                .or_else(|| git(&["rev-parse", "--abbrev-ref", "HEAD"]))
                .unwrap_or_default(),
            runner: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            runs: args.runs,
            warmup: args.warmup,
            runtimes: runtimes
                .iter()
                .map(|r| (r.name().to_string(), r.version.clone()))
                .collect(),
            results,
        }
    }

    /// Print the markdown table to stdout and, under GitHub Actions, append
    /// it to the job summary.
    pub fn print_table(&self) {
        let table = self.render_table();
        println!("{table}");
        if let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY")
            && let Ok(mut f) = fs::OpenOptions::new().append(true).create(true).open(path)
        {
            let _ = f.write_all(table.as_bytes());
        }
    }

    pub fn write(&self, out: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).expect("report serializes");
        fs::write(out, json).map_err(|e| format!("write {}: {e}", out.display()))?;
        eprintln!("wrote {}", out.display());
        Ok(())
    }

    /// A header with commit and runtime versions, then one row per
    /// (workload, runtime); `relative` is against the fastest runtime of
    /// that workload.
    pub fn render_table(&self) -> String {
        let mut s = String::new();
        s += "## purple-garden cross-language benchmarks\n\n";
        s += &format!(
            "commit `{}` ({}{}) · {} · {} runs, {} warmup\n\n",
            self.sha,
            self.date,
            if self.r#ref.is_empty() {
                String::new()
            } else {
                format!(", {}", self.r#ref)
            },
            self.runner,
            self.runs,
            self.warmup
        );
        for (name, ver) in &self.runtimes {
            s += &format!("- `{name}`: {ver}\n");
        }
        s += "\n| workload | runtime | mean ms | ± σ | min ms | cpu ms | peak MiB | relative |\n";
        s += "|---|---|---:|---:|---:|---:|---:|---:|\n";
        let mut by_workload: BTreeMap<&str, Vec<&Measurement>> = BTreeMap::new();
        for m in &self.results {
            by_workload.entry(&m.workload).or_default().push(m);
        }
        for (w, ms) in by_workload {
            let best = ms
                .iter()
                .map(|m| m.time_ms.mean)
                .fold(f64::INFINITY, f64::min);
            for m in ms {
                let mem = m
                    .memory_mb
                    .map(|v| format!("{v:.1}"))
                    .unwrap_or_else(|| "–".into());
                s += &format!(
                    "| {w} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {mem} | {:.2}× |\n",
                    m.runtime,
                    m.time_ms.mean,
                    m.time_ms.stddev,
                    m.time_ms.min,
                    m.cpu_ms,
                    m.time_ms.mean / best
                );
            }
        }
        s
    }
}

/// Rebuild `dir/index.json` from every other `*.json` in `dir`, oldest run
/// first. Returns the number of reports indexed.
pub fn write_index(dir: &Path) -> Result<usize, String> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".json") || name == "index.json" {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let r: Report =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        entries.push(IndexEntry {
            file: name.to_string(),
            sha: r.sha,
            date: r.date,
            run_unix: r.run_unix,
            r#ref: r.r#ref,
        });
    }
    entries.sort_by(|a, b| (a.run_unix, &a.sha).cmp(&(b.run_unix, &b.sha)));
    let json = serde_json::to_string_pretty(&entries).expect("index serializes");
    fs::write(dir.join("index.json"), json).map_err(|e| format!("write index.json: {e}"))?;
    Ok(entries.len())
}

fn git(args: &[&str]) -> Option<String> {
    let o = Command::new("git").args(args).output().ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}
