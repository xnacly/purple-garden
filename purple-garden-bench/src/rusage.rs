//! Peak resident memory of a child process, read from `wait4(2)`.

use std::process::{Command, Stdio};

/// Run `cmd` once and return its peak RSS in MiB.
pub fn peak_rss_mb(cmd: &[String]) -> Result<f64, String> {
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
