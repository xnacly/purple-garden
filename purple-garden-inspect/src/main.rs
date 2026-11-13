//! `purple-garden-inspect <file.garden> [--json] [-o <out>]`: everything compiling the file
//! records, as a self-contained html report (default) or as json, to `out` or stdout.

use std::{io::Write, path::PathBuf, process::exit};

const USAGE: &str = "usage: purple-garden-inspect <file.garden> [--json] [-o <out>]

Compiles the file and writes what every stage recorded: a self-contained html report with the IR
after every optimisation pass, control flow graphs, register allocation, bytecode and x86-64,
or with --json the data behind it.";

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut file, mut out, mut json) = (None, None, false);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json = true,
            "-o" | "--output" => out = args.next().map(PathBuf::from),
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            _ if file.is_none() && !arg.starts_with('-') => file = Some(PathBuf::from(arg)),
            _ => {
                eprintln!("{USAGE}");
                exit(2);
            }
        }
    }
    let Some(file) = file else {
        eprintln!("{USAGE}");
        exit(2);
    };
    let source = std::fs::read_to_string(&file).unwrap_or_else(|e| {
        eprintln!("{}: {e}", file.display());
        exit(1);
    });
    let name = file.file_name().map_or_else(
        || file.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let inspection = purple_garden_inspect::inspect(
        &name,
        source.as_bytes(),
        &purple_garden_shared::config::Config::default(),
    );
    let written = if json {
        serde_json::to_string(&inspection).expect("inspections serialize")
    } else {
        purple_garden_inspect::report::html(&name, &source, &inspection)
    };
    let result = match &out {
        Some(path) => std::fs::write(path, written),
        None => std::io::stdout().lock().write_all(written.as_bytes()),
    };
    if let Err(e) = result {
        eprintln!("writing: {e}");
        exit(1);
    }
}
