use purple_garden_allocators::{
    bump::BumpAlloc,
    metric::MetricAlloc,
    page::{PageAlloc, Pages},
};
use purple_garden_bc as bc;
use purple_garden_frontend::{
    diagnostic::{Diagnostic, Help, Span},
    lex::Lexer,
    lower::Lower,
    parser::Parser,
};
use purple_garden_runtime::{Vm, VmConfig};
use purple_garden_typecheck::Typechecker;

use std::{collections::HashMap, ops::Range, path::Path};

mod cli;
mod doc;
mod elf;
mod frontend;
mod help;
mod lsp;
mod stats;

use cli::{Cli, Command};
use stats::Phases;

pub const BUILD_INFO: &str = concat!(
    "version=",
    env!("CARGO_PKG_VERSION"),
    ";commit=",
    env!("GIT_HASH"),
    ";built=",
    env!("BUILD_TIMESTAMP"),
    ";features=",
    env!("BUILD_FEATURES"),
    ";profile=",
    env!("BUILD_PROFILE"),
    ";opt-level=",
    env!("BUILD_OPT_LEVEL"),
    ";target=",
    env!("BUILD_TARGET"),
    ";rustc=",
    env!("BUILD_RUSTC"),
    ";rustflags=",
    env!("BUILD_RUSTFLAGS"),
);

macro_rules! err {
    ($msg:expr) => {
        Err($msg.into())
    };
}

/// ```text
///         ,            ,            ,    
///     /\^/`\       /\^/`\       /\^/`\   
///    | \/   |     | \/   |     | \/   |  
///    | |    |     | |    |     | |    |  
///    \ \    /     \ \    /     \ \    /  
///     '\\//'       '\\//'       '\\//'   
///       ||           ||           ||     
///       ||           ||           ||     
///       ||           ||           ||     
///       ||  ,        ||  ,        ||  ,  
///   |\  ||  |\   |\  ||  |\   |\  ||  |\
///   | | ||  | |  | | ||  | |  | | ||  | |
///   | | || / /   | | || / /   | | || / /
///    \ \||/ /     \ \||/ /     \ \||/ /  
///     `\\//`       `\\//`       `\\//`   
///    ^^^^^^^^     ^^^^^^^^     ^^^^^^^^  
/// ```
fn entry() -> Result<(), Box<dyn std::error::Error>> {
    let cli = <Cli as clap::Parser>::parse();
    if !cli.any_core {
        purple_garden_shared::sched::prefer_performance_cores();
    }

    match cli.version {
        1 => {
            println!(
                "purple-garden version {} by xnacly and contributors",
                env!("CARGO_PKG_VERSION")
            );
            return Ok(());
        }
        2 => {
            println!(
                "purple-garden version {} by xnacly and contributors",
                env!("CARGO_PKG_VERSION")
            );
            println!("{}", BUILD_INFO.replace(';', "\n"));
            let exe = std::env::current_exe().unwrap();
            println!("from={}", exe.display());
            return Ok(());
        }
        _ => {}
    }

    if let Some(ref cmd) = cli.command {
        match &cmd {
            Command::Check { target } => {
                check_frontend(target, &read_source(target)?, stdlib_packages(&cli))?;
                std::process::exit(0);
            }
            Command::Intro { topic } => {
                println!(
                    "{}",
                    help::print_help_by_topic(topic.as_ref().map(std::string::String::as_str))
                );

                std::process::exit(0);
            }
            Command::Doc { pkg_or_function } => {
                print!(
                    "{}",
                    doc::render_query(pkg_or_function.as_deref())
                        .map_err(Box::<dyn std::error::Error>::from)?
                );
                std::process::exit(0);
            }
            Command::Lsp => return lsp::run(),
        }
    }

    let (source, input_source) = if let Some(ref i) = cli.run {
        (i.clone().into_bytes(), "stdio")
    } else {
        let Some(file_name) = cli.target.as_deref() else {
            return err!("No file or `-r` specified");
        };
        (read_source(file_name)?, file_name)
    };

    if source.is_empty() {
        return Ok(());
    }

    if !cli.alloc_stats {
        return pipeline(&cli, input_source, &source, Phases::<PageAlloc>::default());
    }
    let phases = Phases::<MetricAlloc<PageAlloc>>::default();
    pipeline(&cli, input_source, &source, phases.each_ref())?;
    eprint!("{}", phases.table());
    Ok(())
}

fn pipeline<A: Pages + Clone>(
    cli: &Cli,
    input_source: &str,
    source: &[u8],
    allocs: Phases<A>,
) -> Result<(), Box<dyn std::error::Error>> {
    let conf = &cli.config;

    let arena = BumpAlloc::new_in(allocs.parse.clone());
    // Reset after every phase, so the next one reuses its already touched pages.
    let mut scratch = BumpAlloc::new_in(allocs.scratch);
    let parse = Parser::new(Lexer::new(source), &arena, &scratch).parse_collect();
    scratch.reset();

    let purple_garden_frontend::parser::ParseOutput {
        ast,
        diagnostics: parse_diagnostics,
    } = parse;
    let has_parse_errors = !parse_diagnostics.is_empty();
    let Some(ast) = ast else {
        print_standalone_extern_diagnostic(input_source, source);
        for diagnostic in parse_diagnostics {
            eprintln!("{}", diagnostic.render(input_source, source));
        }
        std::process::exit(1);
    };

    purple_garden_shared::trace!("[main] Tokenisation and Parsing done");

    if cli.ast {
        print!("{ast}");
    }

    let libs = Vec::new();
    let types = BumpAlloc::new_in(allocs.typecheck);
    let typecheck = Typechecker::new(&ast, &types, &scratch)
        .with_libs(libs.clone())
        .with_stdlib(stdlib_packages(cli))
        .check();
    scratch.reset();
    let has_type_errors = !typecheck.diagnostics.is_empty();

    if cli.types > 0 {
        if cli.types == 1 {
            print!("{}", typecheck.render_summary(&ast));
        } else {
            print!("{}", typecheck.render_nodes(&ast));
        }
    }

    let has_frontend_diagnostics = has_parse_errors || has_type_errors;
    if has_frontend_diagnostics {
        print_standalone_extern_diagnostic(input_source, source);
        for diagnostic in parse_diagnostics {
            eprintln!("{}", diagnostic.render(input_source, source));
        }
        for diagnostic in &typecheck.diagnostics {
            eprintln!("{}", diagnostic.clone().render(input_source, source));
        }
        std::process::exit(1);
    }

    let lower = Lower::new_in(&scratch)
        .with_libs(libs)
        .with_stdlib(stdlib_packages(cli));
    let ir_arena = BumpAlloc::new_in(allocs.lower);
    let mut ir = match lower.ir_from_types(&ast, typecheck.types, &ir_arena) {
        Ok(v) => v,
        Err(e) => {
            return err!(e.render(input_source, source));
        }
    };

    scratch.reset();
    purple_garden_shared::trace!("[main] Lowered AST to IR");

    if conf.opt >= 1 {
        purple_garden_opt::ir(&mut ir, &scratch);
        purple_garden_opt::dce_functions(&mut ir, &scratch);
        scratch.reset();
    }

    if cli.ir > 0 {
        let color = purple_garden_shared::ansi::enabled();
        for func in &ir {
            println!("{}", func.pretty(cli.ir > 1, color));
        }
    }

    let mut cc = bc::Cc::new_in(&scratch, allocs.cc);
    let native_pages = cc.compile(conf, &ir, allocs.jit)?;

    purple_garden_shared::trace!("[main] Lowered IR to bytecode");

    if conf.perf_map {
        write_perf_map(cc.native_code.as_deref().unwrap_or_default())?;
    }

    if conf.opt >= 1 {
        purple_garden_opt::bc(&mut cc.buf);
        cc.compact_nops();
    }

    let function_table = if conf.backtrace {
        cc.function_table()
    } else {
        HashMap::new()
    };

    let ctx = if conf.disassemble > 0 {
        Some(cc.clone())
    } else {
        None
    };
    let (mut vm, syscalls, debug, entry_native_idx) = cc.finalize(
        VmConfig {
            backtrace: conf.backtrace,
            no_gc: conf.no_gc,
            stack_size: conf.stack_size,
        },
        allocs.run,
    );
    let entry_native = entry_native_idx.map(|idx| syscalls[idx as usize]);
    let entry = vm.pc;
    // Keep executable JIT pages alive until execution has completed.
    let _native_pages = native_pages;

    if let Some(ctx) = ctx {
        match conf.disassemble {
            1 => {
                // the ops vm.bytecode was encoded from, finalize appends the Halt
                let bytecode: Vec<_> = ctx
                    .buf
                    .iter()
                    .copied()
                    .chain([purple_garden_runtime::op::Op::Halt])
                    .collect();
                let dis = bc::dis::Disassembler::new(
                    &bytecode,
                    &vm.switch_tables,
                    &vm.lookup_tables,
                    ctx,
                )
                .with_source(source);
                dis.disassemble_bytecode();
                dis.disassemble_native();
            }
            _ => elf::write(
                ctx.native_code.as_deref().unwrap_or_default(),
                &elf::addresses(&ctx, &syscalls),
                std::io::stdout().lock(),
            )?,
        }
    }

    if cli.dry {
        return Ok(());
    }

    vm.reset();
    let run = if let Some(entry_native) = entry_native {
        // the entry returns nothing
        unsafe { purple_garden_runtime::enter_native((&mut vm as *mut Vm).cast(), entry_native) };
        vm.take_trap().map_or(Ok(()), Err)
    } else {
        vm.pc = entry;
        if conf.backtrace {
            vm.run::<true>(&syscalls)
        } else {
            vm.run::<false>(&syscalls)
        }
    };

    if let Err(e) = run {
        eprintln!(
            "{}",
            Diagnostic::from_anomaly(e, &debug).render(input_source, source)
        );

        if conf.backtrace {
            if let Some(entry_point_pc) = function_table
                .iter()
                .find(|(_, name)| name.as_str() == "entry")
                .map(|(pc, _)| *pc)
            {
                vm.backtrace.insert(0, entry_point_pc);
            }

            eprintln!("at:");
            for (idx, trace_id) in vm.backtrace.iter().enumerate() {
                if let Some(name) = function_table.get(trace_id) {
                    eprintln!(" #{idx} {name}");
                }
            }
        }

        std::process::exit(1);
    }

    purple_garden_shared::trace!("[main] Executed bytecode");
    Ok(())
}

/// perf's symbol format for jitted code, see tools/perf/Documentation/jit-interface.txt
fn write_perf_map(native_code: &[(&str, usize, Vec<u8>, Vec<Range<usize>>)]) -> std::io::Result<()> {
    use std::io::Write;

    let path = format!("/tmp/perf-{}.map", std::process::id());
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    for (name, entry, code, _) in native_code {
        writeln!(out, "{entry:x} {:x} jit_{name}", code.len())?;
    }
    out.flush()
}

/// The whole source file, read once into memory the process owns. Mapping it instead lets
/// whoever can write the file change or truncate it while the lexer holds `&str`s into it,
/// and pipes like `<(…)` report a length of 0. Lowering keeps source offsets as `u32`, so
/// reading stops past that, which also ends endless inputs like `/dev/zero`.
fn read_source(file_name: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    const MAX_SOURCE: u64 = u32::MAX as u64;
    let failed = |e: std::io::Error| format!("Failed to read '{file_name}': {e}");
    let mut source = Vec::new();
    std::fs::File::open(file_name)
        .and_then(|file| file.take(MAX_SOURCE + 1).read_to_end(&mut source))
        .map_err(failed)?;
    if source.len() as u64 > MAX_SOURCE {
        return Err(format!(
            "Failed to read '{file_name}': larger than {MAX_SOURCE} bytes"
        ));
    }
    Ok(source)
}

fn check_frontend(
    input_source: &str,
    source: &[u8],
    stdlib: &'static [purple_garden_runtime::Pkg],
) -> Result<(), Box<dyn std::error::Error>> {
    let source_path = Path::new(input_source);
    let failed = frontend::analyze_path(
        source_path,
        source,
        Vec::new(),
        stdlib,
        |analysis| {
            for diagnostic in analysis.diagnostics {
                eprintln!(
                    "{}",
                    diagnostic.clone().render(input_source, analysis.source)
                );
            }
            analysis.ast.is_none() || !analysis.diagnostics.is_empty()
        },
    );
    if failed {
        std::process::exit(1);
    }
    Ok(())
}

fn stdlib_packages(cli: &Cli) -> &'static [purple_garden_runtime::Pkg] {
    if cli.no_std {
        return &[];
    }

    if cli.unsafe_std {
        purple_garden_std::STD
    } else {
        purple_garden_std::SAFE_STD
    }
}

fn print_standalone_extern_diagnostic(input_source: &str, source: &[u8]) {
    let source_path = Path::new(input_source);
    let Some(extern_path) = frontend::find_extern_garden(source_path) else {
        return;
    };

    eprintln!(
        "{}",
        Diagnostic::new(
            "Standalone execution ignores nearby extern signatures",
            Span::new(0, first_line_marker_len(source)),
        )
        .with_note(format!(
            "found extern signatures at {}",
            extern_path.display()
        ))
        .with_help(Help::new(format!(
            "use `purple-garden check {input_source}` to validate this file in an embedding context"
        )))
        .render(input_source, source)
    );
}

fn first_line_marker_len(source: &[u8]) -> usize {
    source.iter().position(|&byte| byte == b'\n').unwrap_or(0)
}

fn main() {
    if let Err(e) = entry() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{Cli, read_source, stdlib_packages};
    use clap::Parser;

    fn packages(args: &[&str]) -> Vec<&'static str> {
        let cli = Cli::try_parse_from([&["purple-garden"], args].concat()).unwrap();
        stdlib_packages(&cli).iter().map(|pkg| pkg.name).collect()
    }

    /// Source is untrusted unless the user says otherwise, `unsafe/` reaches into the runtime.
    #[test]
    fn the_unsafe_namespace_is_opt_in() {
        assert!(!packages(&["x.garden"]).contains(&"unsafe"));
        assert!(packages(&["x.garden"]).contains(&"io"));
        assert!(packages(&["--unsafe", "x.garden"]).contains(&"unsafe"));
        assert!(packages(&["--unsafe", "--no-std", "x.garden"]).is_empty());
        assert!(!packages(&["check", "x.garden"]).contains(&"unsafe"));
    }

    fn scratch_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("pg-read-source-{}-{name}", std::process::id()))
    }

    #[test]
    fn reads_a_file_as_it_was_when_read() {
        let path = scratch_path("file.garden");
        std::fs::write(&path, b"import \"io\"\n\xff").unwrap();
        let source = read_source(path.to_str().unwrap()).unwrap();
        // the bytes are ours: truncating the file afterwards changes nothing
        std::fs::write(&path, b"").unwrap();
        assert_eq!(source, b"import \"io\"\n\xff");
        std::fs::remove_file(path).unwrap();
    }

    /// A pipe has no length, mapping it read as an empty program.
    #[test]
    fn reads_a_pipe_to_its_end() {
        let path = scratch_path("fifo");
        let status = std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        let writer = {
            let path = path.clone();
            std::thread::spawn(move || std::fs::write(path, b"io.println(1)\n").unwrap())
        };
        assert_eq!(read_source(path.to_str().unwrap()).unwrap(), b"io.println(1)\n");
        writer.join().unwrap();
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn stops_reading_endless_inputs() {
        let err = read_source("/dev/zero").unwrap_err();
        assert!(err.ends_with("larger than 4294967295 bytes"), "{err}");
    }

    #[test]
    fn reports_directories() {
        assert!(read_source("/").unwrap_err().starts_with("Failed to read '/'"));
    }
}
