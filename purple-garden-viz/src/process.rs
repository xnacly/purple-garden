//! Compiling and running happen in a child process of this binary: a compiler panic (aborting in
//! release), a runaway optimisation or a program that never halts can't take the editor down, and
//! a hang is killed after a timeout.

use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use purple_garden_inspect::Inspection;

/// The hidden subcommand reading a source from stdin and writing its [`Inspection`] as json.
pub const INSPECT: &str = "--inspect";
/// The hidden subcommand reading a source from stdin and running it.
pub const RUN: &str = "--run";

/// What a child left behind.
#[derive(Debug, Clone, Default)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    /// `None` if it was killed or died from a signal.
    pub status: Option<i32>,
    pub elapsed: Duration,
    pub timed_out: bool,
}

/// Runs this binary with `mode`, `source` of the file `name` on its stdin, killing it after
/// `timeout`.
pub fn call(mode: &str, name: &str, source: &str, timeout: Duration) -> std::io::Result<Output> {
    let start = Instant::now();
    let mut child = Command::new(std::env::current_exe()?)
        .args([mode, name])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Read both pipes while waiting, a child filling a pipe nobody drains never exits.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut out = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut out);
            }
            String::from_utf8_lossy(&out).into_owned()
        })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    if let Some(mut stdin) = child.stdin.take() {
        // a child exiting before reading all of it closes the pipe, which isn't an error here
        let _ = stdin.write_all(source.as_bytes());
    }

    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > timeout {
            timed_out = true;
            let _ = child.kill();
            break child.wait()?;
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    Ok(Output {
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
        status: status.code(),
        elapsed: start.elapsed(),
        timed_out,
    })
}

/// [`Inspection`] of `source` of the file `name`, or why the inspecting child failed.
pub fn inspect(name: &str, source: &str) -> Result<Inspection, String> {
    let out = call(INSPECT, name, source, Duration::from_secs(10)).map_err(|e| e.to_string())?;
    if out.timed_out {
        return Err("the compiler did not finish within 10s and was killed".to_string());
    }
    if out.status != Some(0) {
        return Err(format!(
            "the compiler crashed ({}):\n{}",
            out.status
                .map_or("killed by a signal".to_string(), |c| format!("exit {c}")),
            out.stderr
        ));
    }
    serde_json::from_str(&out.stdout).map_err(|e| format!("unreadable inspection: {e}"))
}

fn read_stdin() -> Vec<u8> {
    let mut source = Vec::new();
    std::io::stdin()
        .read_to_end(&mut source)
        .expect("reading the source from stdin");
    source
}

/// The file name the parent passed after the mode, diagnostics are rendered with it.
fn name() -> String {
    std::env::args()
        .nth(2)
        .unwrap_or_else(|| "untitled.garden".to_string())
}

/// The child side of [`INSPECT`].
pub fn serve_inspect() {
    let source = read_stdin();
    let inspection =
        purple_garden_inspect::inspect(&name(), &source, &purple_garden::config::Config::default());
    serde_json::to_writer(std::io::stdout().lock(), &inspection).expect("writing the inspection");
}

/// The child side of [`RUN`], exits 1 with the rendered error if compiling or running fails.
pub fn serve_run() {
    let source = read_stdin();
    let program = purple_garden::Pg::new().with_stdlib().compile(&source);
    let name = name();
    let result = program.map_err(|d| d.render(&name, &source));
    let result = result.and_then(|mut program| {
        program
            .run()
            .map_err(|anomaly| program.diagnostic(anomaly).render(&name, &source))
    });
    if let Err(rendered) = result {
        let _ = std::io::stdout().flush();
        eprint!("{rendered}");
        std::process::exit(1);
    }
}
