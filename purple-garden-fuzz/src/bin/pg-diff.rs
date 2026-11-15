//! Explore seeds outside of libFuzzer, classify and minimise what fails:
//!
//! ```sh
//! pg-diff run <start> <count>   # check seeds, print the verdict of every failing one
//! pg-diff show <seed>           # print the program of a seed
//! pg-diff check <file>          # every backend's outcome for a program file
//! pg-diff verdict <file>        # one line classifying a program file, see `verdict`
//! pg-diff reduce <file> <out>   # minimise a program keeping its verdict, see `reduce`
//! ```
//!
//! `run` checks seeds in process: a crash (an abort, a fault) ends it, the last `seed N` line
//! on stderr names the culprit. `reduce` runs every candidate in a child process, so it
//! minimises crashes too.

use std::{panic, process::Command};

use purple_garden_fuzz::{oracle, seeded};

const BYTES: usize = 2048;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let num = |i: usize| -> u64 {
        args.get(i)
            .and_then(|a| a.parse().ok())
            .unwrap_or_else(|| panic!("argument {i} must be a number"))
    };
    let read = |i: usize| std::fs::read_to_string(&args[i]).expect("readable program");
    match args.get(1).map(String::as_str) {
        Some("show") => print!("{}", seeded(num(2), BYTES)),
        Some("check") => {
            let src = read(2);
            for backend in oracle::BACKENDS {
                println!("{backend}: {:?}", backend.run(&src));
            }
        }
        Some("verdict") => {
            panic::set_hook(Box::new(|_| {}));
            println!("{}", verdict(&read(2)));
        }
        Some("reduce") => {
            let reduced = reduce(&read(2));
            std::fs::write(&args[3], &reduced).expect("writable output");
            print!("{reduced}");
        }
        Some("run") => {
            panic::set_hook(Box::new(|_| {}));
            let (start, count) = (num(2), num(3));
            let mut failed = 0;
            for seed in start..start + count {
                eprintln!("seed {seed}");
                let verdict = verdict(&seeded(seed, BYTES));
                if verdict != "ok" {
                    failed += 1;
                    println!("seed {seed}: {verdict}");
                }
            }
            println!("{failed} of {count} seeds failed");
        }
        _ => eprintln!(
            "usage: pg-diff run <start> <count> | show <seed> | check <file> | verdict <file> | reduce <file> <out>"
        ),
    }
}

/// `ok`, or what went wrong: `rejected` (the reference refuses the program, a generator bug
/// for generated ones), `panic <message>`, or `diverge <backend> <outcome kind>`.
fn verdict(src: &str) -> String {
    let kind = |o: &oracle::Outcome| match o {
        oracle::Outcome::Value(_) => "value".to_string(),
        oracle::Outcome::Anomaly(a) => format!("anomaly {a}"),
        oracle::Outcome::Rejected(_) => "rejected".to_string(),
    };
    match panic::catch_unwind(|| oracle::check(src)) {
        Ok(Ok(oracle::Outcome::Rejected(d))) => format!("rejected {d}"),
        Ok(Ok(_)) => "ok".into(),
        Ok(Err(d)) => format!(
            "diverge {} {} vs {} ({d})",
            d.backend,
            kind(&d.actual),
            kind(&d.expected)
        ),
        Err(panic) => format!(
            "panic {}",
            panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_default()
        ),
    }
}

/// The verdict of `src` from a child process: a crash is a verdict too.
fn child_verdict(src: &str) -> String {
    let path = std::env::temp_dir().join(format!("pg-reduce-{}.garden", std::process::id()));
    std::fs::write(&path, src).expect("writable temp file");
    let out = Command::new(std::env::current_exe().expect("own path"))
        .arg("verdict")
        .arg(&path)
        .output()
        .expect("child runs");
    match out.status.code() {
        Some(0) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        code => format!("crash {code:?}"),
    }
}

/// The part of a verdict that has to stay while reducing: a value divergence keeps diverging
/// on the same backend, but its values change. With `PG_REDUCE_ANY_BACKEND` set any backend
/// may diverge, for nondeterministic programs.
fn signature(verdict: &str) -> String {
    let verdict = verdict.split(" (").next().unwrap_or(verdict);
    if std::env::var_os("PG_REDUCE_ANY_BACKEND").is_some()
        && let Some(rest) = verdict.strip_prefix("diverge ")
    {
        return format!(
            "diverge{}",
            rest.split_once(" jit")
                .or(rest.split_once(" vm"))
                .map_or("", |(_, r)| r)
        );
    }
    verdict.to_string()
}

/// Greedy minimisation keeping the [`signature`] of the verdict: drop line ranges (halving
/// their length down to single lines), then replace parenthesised expressions and words by
/// literals, until nothing changes.
fn reduce(src: &str) -> String {
    let goal = signature(&child_verdict(src));
    assert!(goal != "ok", "nothing to reduce, the program passes");
    eprintln!("keeping: {goal}");
    let keeps = |candidate: &str| signature(&child_verdict(candidate)) == goal;
    let mut best = src.to_string();
    loop {
        let before = best.clone();
        best = drop_lines(best, &keeps);
        best = replace_exprs(best, &keeps);
        if best == before {
            return best;
        }
    }
}

fn drop_lines(mut best: String, keeps: &impl Fn(&str) -> bool) -> String {
    let mut chunk = best.lines().count().max(1);
    while chunk >= 1 {
        let mut i = 0;
        loop {
            let lines: Vec<&str> = best.lines().collect();
            if i >= lines.len() {
                break;
            }
            let end = (i + chunk).min(lines.len());
            let candidate: String = lines[..i]
                .iter()
                .chain(&lines[end..])
                .map(|l| format!("{l}\n"))
                .collect();
            if keeps(&candidate) {
                eprintln!("dropped lines {i}..{end}, {} left", lines.len() - (end - i));
                best = candidate;
            } else {
                i += chunk;
            }
        }
        chunk /= 2;
    }
    best
}

const LITERALS: &[&str] = &["0", "1", "0.0", "true", "\"\"", "\"a\""];

/// Replaces each parenthesised group or word with each of [`LITERALS`], the first that keeps
/// the verdict wins.
fn replace_exprs(mut best: String, keeps: &impl Fn(&str) -> bool) -> String {
    let mut start = 0;
    while start < best.len() {
        let bytes = best.as_bytes();
        let end = match bytes[start] {
            b'(' | b'[' | b'{' => matching(bytes, start),
            c if c.is_ascii_alphanumeric() && (start == 0 || !is_word(bytes[start - 1])) => {
                let mut end = start;
                while end < bytes.len() && (is_word(bytes[end]) || bytes[end] == b'.') {
                    end += 1;
                }
                Some(end)
            }
            _ => None,
        };
        let Some(end) = end else {
            start += 1;
            continue;
        };
        let original = &best[start..end];
        let replaced = LITERALS.iter().find_map(|lit| {
            if *lit == original {
                return None;
            }
            let candidate = format!("{}{lit}{}", &best[..start], &best[end..]);
            keeps(&candidate).then_some(candidate)
        });
        match replaced {
            Some(candidate) => {
                eprintln!("replaced {original:.40}");
                best = candidate;
            }
            None => start += 1,
        }
    }
    best
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn matching(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_str = false;
    for (i, &c) in bytes.iter().enumerate().skip(open) {
        match c {
            b'"' => in_str = !in_str,
            b'(' | b'[' | b'{' if !in_str => depth += 1,
            b')' | b']' | b'}' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}
