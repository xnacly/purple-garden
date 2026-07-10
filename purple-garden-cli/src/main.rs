use purple_garden_bc as bc;
use purple_garden_frontend::{
    diagnostic::{Diagnostic, Help, Span},
    lex::Lexer,
    lower::Lower,
    parser::Parser,
    typecheck::Typechecker,
};
use purple_garden_runtime::VmConfig;

use std::{collections::HashMap, path::Path};

mod cli;
mod doc;
mod elf;
mod frontend;
mod help;
mod input;
mod lsp;

use cli::{Cli, Command};
use input::Input;

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
    let conf = &cli.config;

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
                let input = Input::from_file(target)?;
                check_frontend(target, &input, !conf.no_std)?;
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

    let (input, input_source) = if let Some(ref i) = cli.run {
        (Input::Str(i.clone()), "stdio")
    } else {
        let Some(file_name) = cli.target.as_deref() else {
            return err!("No file or `-r` specified");
        };
        (Input::from_file(file_name)?, file_name)
    };

    if input.is_empty() {
        return Ok(());
    }

    let source = input.as_bytes();

    let parse = Parser::new(Lexer::new(source)).parse_collect();
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
    let typecheck = Typechecker::new(&ast)
        .with_libs(libs.clone())
        .with_stdlib_enabled(!conf.no_std)
        .check();
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

    let lower = Lower::new()
        .with_libs(libs)
        .with_stdlib_enabled(!conf.no_std);
    let mut ir = match lower.ir_from_types(&ast, typecheck.types) {
        Ok(v) => v,
        Err(e) => {
            return err!(e.render(input_source, source));
        }
    };

    purple_garden_shared::trace!("[main] Lowered AST to IR");

    if conf.opt >= 1 {
        purple_garden_opt::ir(&mut ir);
    }

    if cli.ir {
        for func in &ir {
            println!("{func}");
        }
    }

    let mut cc = bc::Cc::new();
    let native_pages = cc.compile(conf, &ir)?;

    purple_garden_shared::trace!("[main] Lowered IR to bytecode");

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
    let (vm, syscalls, debug, entry_native_idx) = cc.finalize(VmConfig {
        backtrace: conf.backtrace,
        no_gc: conf.no_gc,
    });
    let entry_native = entry_native_idx.map(|idx| syscalls[idx as usize]);
    let mut program =
        purple_garden::Program::from_vm(vm, syscalls, debug).with_entry_native(entry_native);
    if !conf.no_jit {
        program.jit = native_pages;
    }

    if let Some(ctx) = ctx {
        match conf.disassemble {
            1 => {
                let dis = bc::dis::Disassembler::new(&program.vm.bytecode, ctx).with_source(source);
                dis.disassemble_bytecode();
                dis.disassemble_native();
            }
            _ => elf::write(
                ctx.native_code.as_deref().unwrap_or_default(),
                std::io::stdout().lock(),
            )?,
        }
    }

    if cli.dry {
        return Ok(());
    }

    if let Err(e) = program.run() {
        eprintln!(
            "{}",
            Diagnostic::from_anomaly(e, &program.debug).render(input_source, source)
        );

        if conf.backtrace {
            if let Some(entry_point_pc) = function_table
                .iter()
                .find(|(_, name)| name.as_str() == "entry")
                .map(|(pc, _)| *pc)
            {
                program.vm.backtrace.insert(0, entry_point_pc);
            }

            eprintln!("at:");
            for (idx, trace_id) in program.vm.backtrace.iter().enumerate() {
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

fn check_frontend(
    input_source: &str,
    input: &Input,
    stdlib: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let source_path = Path::new(input_source);
    let failed = frontend::analyze_path(
        source_path,
        input.as_bytes(),
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
