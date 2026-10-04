//! Cross-language benchmark runner: purple-garden against CPython,
//! JavaScriptCore (bun) and LuaJIT, measured with hyperfine.
//!
//! `run` benchmarks every workload under `bench/cross/workloads` on every
//! available runtime and writes a JSON report plus a markdown table; `index`
//! rebuilds the `index.json` the published site reads. Each concern has its
//! own module: [`runtime`] (what to run), [`workload`] (what to run it on),
//! [`hyperfine`] (wall and CPU time), [`rusage`] (peak memory), [`report`]
//! (output and the site index).

mod hyperfine;
mod report;
mod runtime;
mod rusage;
mod workload;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::{
    report::{Measurement, Report},
    runtime::Runtime,
    workload::Workload,
};

#[derive(Parser)]
#[command(
    about = "Benchmark purple-garden against CPython, JavaScriptCore (bun) and LuaJIT with hyperfine"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Benchmark every workload on every available runtime.
    Run(RunArgs),
    /// Rebuild `<dir>/index.json` from the reports in `<dir>`.
    Index {
        /// Directory holding `<sha>.json` reports written by `run --out`.
        dir: PathBuf,
    },
}

#[derive(clap::Args)]
pub struct RunArgs {
    /// Directory with one sub-directory per workload.
    #[arg(long, default_value = "bench/cross/workloads")]
    pub workloads: PathBuf,
    /// purple-garden binary under test.
    #[arg(long, default_value = "target/release/purple-garden")]
    pub garden: PathBuf,
    /// Measured runs per command.
    #[arg(long, default_value_t = 10)]
    pub runs: u32,
    /// Unmeasured warmup runs per command.
    #[arg(long, default_value_t = 3)]
    pub warmup: u32,
    /// Run only this workload.
    #[arg(long)]
    pub only: Option<String>,
    /// Write the JSON report here.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Error on a missing runtime binary instead of skipping it.
    #[arg(long)]
    pub strict: bool,
    /// Skip the peak-memory pass.
    #[arg(long)]
    pub no_memory: bool,
}

fn main() {
    let result = match Cli::parse().cmd {
        Cmd::Run(args) => run(&args),
        Cmd::Index { dir } => {
            report::write_index(&dir).map(|n| eprintln!("indexed {n} reports"))
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(args: &RunArgs) -> Result<(), String> {
    let garden = args.garden.canonicalize().map_err(|e| {
        format!(
            "garden binary {}: {e} (build it with `cargo build --release -p purple-garden-cli`)",
            args.garden.display()
        )
    })?;
    let runtimes = runtime::resolve(&garden, args.strict)?;
    let workloads = workload::discover(&args.workloads, args.only.as_deref())?;
    workload::preflight(&workloads, &runtimes)?;

    let mut results = Vec::new();
    for w in &workloads {
        eprintln!("== {} ==", w.name);
        results.extend(measure(w, &runtimes, args)?);
    }

    let report = Report::new(args, &runtimes, results);
    report.print_table();
    if let Some(out) = &args.out {
        report.write(out)?;
    }
    Ok(())
}

/// Time every runtime on one workload with hyperfine, then measure peak RSS
/// with one extra run per runtime.
fn measure(
    w: &Workload,
    runtimes: &[Runtime],
    args: &RunArgs,
) -> Result<Vec<Measurement>, String> {
    let mut measured = hyperfine::measure(w, runtimes, args.runs, args.warmup)?;
    if !args.no_memory {
        for m in &mut measured {
            let rt = runtimes
                .iter()
                .find(|r| r.name() == m.runtime)
                .expect("hyperfine labels are runtime names");
            m.memory_mb = Some(rusage::peak_rss_mb(&w.command(rt))?);
        }
    }
    Ok(measured)
}
