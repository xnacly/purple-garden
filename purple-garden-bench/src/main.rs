//! Cross-language benchmark runner.
//!
//! `run` walks `bench/cross/workloads/*/`, where every directory holds one
//! program in four languages (`main.garden`, `main.js`, `main.lua`,
//! `main.py`), and drives hyperfine over purple-garden, bun (JavaScriptCore),
//! luajit and python3 (CPython). Wall and CPU time come from hyperfine's JSON
//! export; peak resident memory from one extra run per command read out of
//! `wait4(2)`'s rusage, so no `/usr/bin/time` is needed.
//!
//! Every program exits non-zero on a wrong result and hyperfine aborts on a
//! non-zero exit, so a broken port cannot silently produce a number. A
//! preflight pass runs each program once first so that failure is readable.
//!
//! `run` prints a markdown table (also appended to `$GITHUB_STEP_SUMMARY`
//! when set) and, with `--out`, writes one JSON report with an entry per
//! (workload, runtime). `index` rebuilds the `index.json` the published site
//! reads from a directory of such reports.

use std::{
    collections::BTreeMap,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(
    about = "Benchmark purple-garden against CPython, JavaScriptCore (bun) and LuaJIT with hyperfine"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Benchmark every workload on every available runtime.
    Run(RunArgs),
    /// Rebuild `<dir>/index.json` from the reports in `<dir>`.
    Index {
        /// Directory holding `<sha>.json` reports written by `run --out`.
        dir: PathBuf,
    },
}

#[derive(clap::Args, Debug)]
struct RunArgs {
    /// Directory with one sub-directory per workload.
    #[arg(long, default_value = "bench/cross/workloads")]
    workloads: PathBuf,
    /// purple-garden binary under test.
    #[arg(long, default_value = "target/release/purple-garden")]
    garden: PathBuf,
    /// Measured runs per command.
    #[arg(long, default_value_t = 10)]
    runs: u32,
    /// Unmeasured warmup runs per command.
    #[arg(long, default_value_t = 3)]
    warmup: u32,
    /// Run only this workload.
    #[arg(long)]
    only: Option<String>,
    /// Write the JSON report here.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Error on a missing runtime binary instead of skipping it.
    #[arg(long)]
    strict: bool,
    /// Skip the peak-memory pass.
    #[arg(long)]
    no_memory: bool,
}

/// How to invoke one runtime. `bin == None` means the `--garden` binary.
struct RuntimeSpec {
    name: &'static str,
    bin: Option<&'static str>,
    source: &'static str,
    version_args: &'static [&'static str],
}

const RUNTIMES: &[RuntimeSpec] = &[
    RuntimeSpec {
        name: "garden",
        bin: None,
        source: "main.garden",
        version_args: &["-V"],
    },
    RuntimeSpec {
        name: "bun",
        bin: Some("bun"),
        source: "main.js",
        version_args: &["--version"],
    },
    RuntimeSpec {
        name: "luajit",
        bin: Some("luajit"),
        source: "main.lua",
        version_args: &["-v"],
    },
    RuntimeSpec {
        name: "python3",
        bin: Some("python3"),
        source: "main.py",
        version_args: &["--version"],
    },
];

/// A runtime that resolved on this machine.
struct Runtime {
    spec: &'static RuntimeSpec,
    bin: String,
    version: String,
}

struct Workload {
    name: String,
    dir: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct Report {
    sha: String,
    /// Commit date (ISO-8601), so a history plots in commit order.
    date: String,
    run_unix: u64,
    /// Branch or tag the run was made from (`GITHUB_REF_NAME` in CI).
    #[serde(default)]
    r#ref: String,
    runner: String,
    runs: u32,
    warmup: u32,
    runtimes: BTreeMap<String, String>,
    results: Vec<Measurement>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Measurement {
    workload: String,
    runtime: String,
    time_ms: TimeStats,
    cpu_ms: f64,
    memory_mb: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct TimeStats {
    mean: f64,
    min: f64,
    stddev: f64,
}

/// One row of `index.json`: enough to order and fetch a report.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct IndexEntry {
    file: String,
    sha: String,
    date: String,
    run_unix: u64,
    #[serde(default)]
    r#ref: String,
}

/// The slice of hyperfine's `--export-json` schema this runner reads. Times
/// are seconds; `stddev` is null for a single run.
#[derive(Deserialize)]
struct HyperfineExport {
    results: Vec<HyperfineResult>,
}

#[derive(Deserialize)]
struct HyperfineResult {
    /// The `-n` label when one is given.
    command: String,
    mean: f64,
    stddev: Option<f64>,
    min: f64,
    user: f64,
    system: f64,
}

fn main() {
    let cli = Cli::parse();
    let result = match &cli.cmd {
        Cmd::Run(args) => run(args),
        Cmd::Index { dir } => write_index(dir).map(|n| eprintln!("indexed {n} reports")),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(cli: &RunArgs) -> Result<(), String> {
    let garden = cli.garden.canonicalize().map_err(|e| {
        format!(
            "garden binary {}: {e} (build it with `cargo build --release -p purple-garden-cli`)",
            cli.garden.display()
        )
    })?;
    let runtimes = resolve_runtimes(cli, &garden)?;
    let workloads = discover_workloads(&cli.workloads, cli.only.as_deref())?;
    preflight(&workloads, &runtimes)?;

    let mut results = Vec::new();
    for w in &workloads {
        eprintln!("== {} ==", w.name);
        let mut measured = hyperfine(w, &runtimes, cli)?;
        if !cli.no_memory {
            for m in &mut measured {
                let rt = runtimes
                    .iter()
                    .find(|r| r.spec.name == m.runtime)
                    .expect("hyperfine label is a runtime name");
                m.memory_mb = Some(peak_rss_mb(&command_for(w, rt))?);
            }
        }
        results.extend(measured);
    }

    let report = Report {
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
        runs: cli.runs,
        warmup: cli.warmup,
        runtimes: runtimes
            .iter()
            .map(|r| (r.spec.name.to_string(), r.version.clone()))
            .collect(),
        results,
    };

    let table = render_table(&report);
    println!("{table}");
    if let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY")
        && let Ok(mut f) = fs::OpenOptions::new().append(true).create(true).open(path)
    {
        let _ = f.write_all(table.as_bytes());
    }
    if let Some(out) = &cli.out {
        let json = serde_json::to_string_pretty(&report).expect("report serializes");
        fs::write(out, json).map_err(|e| format!("write {}: {e}", out.display()))?;
        eprintln!("wrote {}", out.display());
    }
    Ok(())
}

/// Rebuild `dir/index.json` from every other `*.json` in `dir`, oldest run
/// first. Returns the number of reports indexed.
fn write_index(dir: &Path) -> Result<usize, String> {
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

fn resolve_runtimes(cli: &RunArgs, garden: &Path) -> Result<Vec<Runtime>, String> {
    let mut out = Vec::new();
    for spec in RUNTIMES {
        let bin = spec
            .bin
            .map(str::to_string)
            .unwrap_or_else(|| garden.to_string_lossy().into_owned());
        match Command::new(&bin).args(spec.version_args).output() {
            Ok(o) => {
                let text = if o.stdout.is_empty() { o.stderr } else { o.stdout };
                let version = String::from_utf8_lossy(&text)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                out.push(Runtime { spec, bin, version });
            }
            Err(e) if cli.strict => {
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

fn discover_workloads(dir: &Path, only: Option<&str>) -> Result<Vec<Workload>, String> {
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

/// `[program, source_file]` for one (workload, runtime).
fn command_for(w: &Workload, rt: &Runtime) -> Vec<String> {
    vec![
        rt.bin.clone(),
        w.dir.join(rt.spec.source).to_string_lossy().into_owned(),
    ]
}

/// Run every program once so a wrong result fails with a readable message
/// rather than hyperfine's generic non-zero-exit abort.
fn preflight(workloads: &[Workload], runtimes: &[Runtime]) -> Result<(), String> {
    let mut failures = Vec::new();
    for w in workloads {
        for rt in runtimes {
            let cmd = command_for(w, rt);
            if !Path::new(&cmd[1]).is_file() {
                failures.push(format!("{}/{}: missing {}", w.name, rt.spec.name, cmd[1]));
                continue;
            }
            let status = Command::new(&cmd[0])
                .args(&cmd[1..])
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .status();
            match status {
                Ok(s) if s.success() => {}
                Ok(s) => failures.push(format!(
                    "{}/{}: exit {}",
                    w.name,
                    rt.spec.name,
                    s.code().unwrap_or(-1)
                )),
                Err(e) => failures.push(format!("{}/{}: {e}", w.name, rt.spec.name)),
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

fn hyperfine(w: &Workload, runtimes: &[Runtime], cli: &RunArgs) -> Result<Vec<Measurement>, String> {
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
        &cli.warmup.to_string(),
        "--runs",
        &cli.runs.to_string(),
        "--export-json",
    ])
    .arg(&export);
    for rt in runtimes {
        let parts = command_for(w, rt);
        if parts.iter().any(|p| p.chars().any(char::is_whitespace)) {
            return Err(format!(
                "paths with whitespace are not supported with hyperfine -N: {parts:?}"
            ));
        }
        cmd.args(["-n", rt.spec.name]).arg(parts.join(" "));
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
    parse_hyperfine(&w.name, &text)
}

fn parse_hyperfine(workload: &str, json: &str) -> Result<Vec<Measurement>, String> {
    let export: HyperfineExport =
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

/// Peak resident set size of one run, in MiB, read from `wait4(2)`.
fn peak_rss_mb(cmd: &[String]) -> Result<f64, String> {
    let child = Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{}: {e}", cmd[0]))?;
    let pid = child.id() as libc::pid_t;
    let mut status = 0i32;
    // SAFETY: `rusage` is plain data. wait4 fills it for the child we just
    // spawned and reaps it; std's `Child` does not wait on drop, so there is
    // no double reap.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::wait4(pid, &mut status, 0, &mut usage) };
    drop(child);
    if rc < 0 {
        return Err(format!("wait4 failed for {}", cmd[0]));
    }
    // ru_maxrss is kilobytes on Linux and bytes on macOS.
    let kib = if cfg!(target_os = "macos") {
        usage.ru_maxrss as f64 / 1024.0
    } else {
        usage.ru_maxrss as f64
    };
    Ok(kib / 1024.0)
}

fn git(args: &[&str]) -> Option<String> {
    let o = Command::new("git").args(args).output().ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn render_table(r: &Report) -> String {
    let mut s = String::new();
    s += "## purple-garden cross-language benchmarks\n\n";
    s += &format!(
        "commit `{}` ({}{}) · {} · {} runs, {} warmup\n\n",
        r.sha,
        r.date,
        if r.r#ref.is_empty() { String::new() } else { format!(", {}", r.r#ref) },
        r.runner,
        r.runs,
        r.warmup
    );
    for (name, ver) in &r.runtimes {
        s += &format!("- `{name}`: {ver}\n");
    }
    s += "\n| workload | runtime | mean ms | ± σ | min ms | cpu ms | peak MiB | relative |\n";
    s += "|---|---|---:|---:|---:|---:|---:|---:|\n";
    let mut by_workload: BTreeMap<&str, Vec<&Measurement>> = BTreeMap::new();
    for m in &r.results {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn report(sha: &str, run_unix: u64) -> Report {
        Report {
            sha: sha.into(),
            date: "2026-10-04T00:00:00Z".into(),
            run_unix,
            r#ref: "master".into(),
            runner: "test".into(),
            runs: 1,
            warmup: 0,
            runtimes: BTreeMap::new(),
            results: vec![],
        }
    }

    #[test]
    fn parses_hyperfine_export_and_converts_to_ms() {
        let json = r#"{"results":[
            {"command":"garden","mean":0.0022,"stddev":0.0001,"median":0.0022,"user":0.0004,"system":0.0012,"min":0.0021,"max":0.0024,"times":[0.0022],"exit_codes":[0]},
            {"command":"luajit","mean":0.0013,"stddev":null,"median":0.0013,"user":0.0007,"system":0.0006,"min":0.0013,"max":0.0013,"times":[0.0013],"exit_codes":[0]}
        ]}"#;
        let ms = parse_hyperfine("gcd_sum", json).unwrap();
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0].runtime, "garden");
        assert!((ms[0].time_ms.mean - 2.2).abs() < 1e-9);
        assert!((ms[0].cpu_ms - 1.6).abs() < 1e-9);
        assert_eq!(ms[1].time_ms.stddev, 0.0, "null stddev reads as 0");
        assert_eq!(ms[0].memory_mb, None);
    }

    #[test]
    fn table_marks_fastest_as_1x() {
        let mut r = report("abc", 0);
        r.r#ref.clear();
        r.results = vec![
            Measurement {
                workload: "w".into(),
                runtime: "a".into(),
                time_ms: TimeStats { mean: 2.0, min: 2.0, stddev: 0.0 },
                cpu_ms: 1.0,
                memory_mb: Some(1.0),
            },
            Measurement {
                workload: "w".into(),
                runtime: "b".into(),
                time_ms: TimeStats { mean: 1.0, min: 1.0, stddev: 0.0 },
                cpu_ms: 1.0,
                memory_mb: None,
            },
        ];
        let t = render_table(&r);
        assert!(t.contains("| w | a | 2.00 | 0.00 | 2.00 | 1.00 | 1.0 | 2.00× |"));
        assert!(t.contains("| w | b | 1.00 | 0.00 | 1.00 | 1.00 | – | 1.00× |"));
    }

    #[test]
    fn index_orders_reports_by_run_time_and_skips_itself() {
        let dir = std::env::temp_dir().join(format!("pg-bench-index-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for (sha, t) in [("bbb", 200u64), ("aaa", 100)] {
            fs::write(
                dir.join(format!("{sha}.json")),
                serde_json::to_string(&report(sha, t)).unwrap(),
            )
            .unwrap();
        }
        fs::write(dir.join("index.json"), "[]").unwrap();
        fs::write(dir.join("notes.txt"), "ignored").unwrap();

        assert_eq!(write_index(&dir).unwrap(), 2);
        let index: Vec<IndexEntry> =
            serde_json::from_str(&fs::read_to_string(dir.join("index.json")).unwrap()).unwrap();
        assert_eq!(
            index.iter().map(|e| e.sha.as_str()).collect::<Vec<_>>(),
            ["aaa", "bbb"],
            "oldest run first"
        );
        assert_eq!(index[0].file, "aaa.json");
        assert_eq!(index[0].r#ref, "master");
        let _ = fs::remove_dir_all(&dir);
    }
}
