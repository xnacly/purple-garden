use purple_garden::{help, input::Input};
use purple_garden_bc as bc;
use purple_garden_frontend::{
    diagnostic::Diagnostic, lex::Lexer, lower::Lower, parser::Parser, typecheck::Typechecker,
};
use purple_garden_runtime::VmConfig;
use purple_garden_shared::config;
use purple_garden_std::{self as pstd, Pkg};
use std::collections::HashMap;

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

fn entry() -> Result<(), Box<dyn std::error::Error>> {
    let conf = <config::Config as clap::Parser>::parse();
    match conf.version {
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

    if let Some(ref cmd) = conf.command {
        match &cmd {
            config::Command::Intro { topic } => {
                println!(
                    "{}",
                    help::print_help_by_topic(topic.as_ref().map(std::string::String::as_str))
                );

                std::process::exit(0);
            }
            config::Command::Doc { pkg_or_function } => {
                // with no argument we just print all stdlib packages
                let Some(pkg_or_function) = pkg_or_function else {
                    fn print_pkg(pkg: &Pkg) {
                        println!("{}", pkg.name);
                        for sub in pkg.pkgs {
                            print_pkg(sub);
                        }
                    }
                    println!("Purple garden standard library packages:");
                    for pkg in purple_garden_std::STD {
                        print_pkg(pkg);
                    }
                    std::process::exit(0);
                };

                let (path, method) = match pkg_or_function.split_once('.') {
                    Some((path, method)) => (path, Some(method)),
                    None => (pkg_or_function.as_str(), None),
                };

                let Some(pkg) = pstd::resolve_pkg(path) else {
                    return err!(format!("query {} couldnt be resolved to anything", path));
                };

                if let Some(method) = method {
                    let Some((name, variants)) = pkg
                        .overload_groups()
                        .into_iter()
                        .find(|(name, _)| *name == method)
                    else {
                        return err!(format!("function {}.{} not found", pkg.name, method));
                    };

                    let mut out = String::new();
                    purple_garden_runtime::print_overload_group(name, &variants, &mut out)
                        .expect("writing to a String cannot fail");
                    print!("{out}");
                } else {
                    println!("{pkg}");
                }

                std::process::exit(0);
            }
        }
    }

    let (input, input_source) = if let Some(ref i) = conf.run {
        (Input::Str(i.clone()), "stdio")
    } else {
        let Some(file_name) = conf.target.as_deref() else {
            return err!("No file or `-r` specified");
        };
        (Input::from_file(file_name)?, file_name)
    };

    if input.is_empty() {
        return Ok(());
    }

    let lexer = Lexer::new(input.as_bytes());
    let ast = match Parser::new(lexer).and_then(purple_garden_frontend::parser::Parser::parse) {
        Ok(a) => a,
        Err(e) => {
            return err!(e.render(input_source, input.as_bytes()));
        }
    };

    purple_garden_shared::trace!("[main] Tokenisation and Parsing done");

    if conf.ast {
        print!("{ast}");
    }

    let libs = Vec::new();
    let typecheck = Typechecker::new(&ast).with_libs(libs.clone()).check();
    let has_type_errors = !typecheck.diagnostics.is_empty();
    if has_type_errors {
        for diagnostic in &typecheck.diagnostics {
            eprintln!(
                "{}",
                diagnostic.clone().render(input_source, input.as_bytes())
            );
        }
    }
    if conf.types > 0 {
        if conf.types == 1 {
            print!("{}", typecheck.render_summary(&ast));
        } else {
            print!("{}", typecheck.render_nodes(&ast));
        }
        if has_type_errors {
            std::process::exit(1);
        }

        let needs_lowered_output = conf.ir || conf.liveness || conf.disassemble > 0;
        if !needs_lowered_output {
            std::process::exit(0);
        }
    }
    if has_type_errors {
        std::process::exit(1);
    }

    let lower = Lower::new().with_libs(libs);
    let mut ir = match lower.ir_from_types(&ast, typecheck.types) {
        Ok(v) => v,
        Err(e) => {
            return err!(e.render(input_source, input.as_bytes()));
        }
    };

    purple_garden_shared::trace!("[main] Lowered AST to IR");

    if conf.opt >= 1 {
        purple_garden_opt::ir(&mut ir);
    }

    if conf.ir {
        for func in &ir {
            println!("{func}");
        }
    }

    let mut cc = bc::Cc::new();
    let native_pages = cc.compile(&conf, &ir)?;

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
    });
    let entry_native = entry_native_idx.map(|idx| syscalls[idx as usize]);
    let mut program =
        purple_garden::Program::from_vm(vm, syscalls, debug).with_entry_native(entry_native);
    if !conf.no_jit {
        program.jit = native_pages;
    }

    if let Some(ctx) = ctx {
        let dis =
            bc::dis::Disassembler::new(&program.vm.bytecode, ctx).with_source(input.as_bytes());
        match conf.disassemble {
            1 => {
                dis.disassemble_bytecode();
                dis.disassemble_native();
            }
            _ => dis.dump_native_elf(std::io::stdout().lock())?,
        }
    }

    if conf.dry {
        return Ok(());
    }

    if let Err(e) = program.run() {
        eprintln!(
            "{}",
            Diagnostic::from_anomaly(e, &program.debug).render(input_source, input.as_bytes())
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

fn main() {
    if let Err(e) = entry() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
