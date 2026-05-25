use purple_garden::{config, help, input::Input, trace};
use purple_garden_bc as bc;
use purple_garden_frontend::{err::PgError, lex::Lexer, lower::Lower, parser::Parser};
use purple_garden_runtime::VmConfig;
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
                    let Some(fun) = pkg.fns.iter().find(|f| f.name == method) else {
                        return err!(format!("function {}.{} not found", pkg.name, method));
                    };
                    println!("{fun}");
                } else {
                    println!("{pkg}");
                }

                std::process::exit(0);
            }
        }
    }

    let (input, input_source) = if let Some(ref i) = conf.run { (Input::Str(i.clone()), "stdio") } else {
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

    trace!("[main] Tokenisation and Parsing done");

    if conf.ast {
        print!(
            "{}",
            ast.iter()
                .map(std::string::ToString::to_string)
                .collect::<String>()
        );
    }

    let lower = Lower::new();
    let (mut ir, pkg_fns) = match lower.ir_from(&ast) {
        Ok(v) => v,
        Err(e) => {
            return err!(e.render(input_source, input.as_bytes()));
        }
    };

    trace!("[main] Lowered AST to IR");

    if conf.opt >= 1 {
        purple_garden_opt::ir(&mut ir);
    }

    if conf.ir {
        for func in &ir {
            println!("{func}");
        }
    }

    let mut cc = bc::Cc::new();
    cc.compile(conf.liveness, &ir, &pkg_fns);

    trace!("[main] Lowered IR to bytecode");

    if conf.opt >= 1 {
        purple_garden_opt::bc(&mut cc.buf);
        cc.compact_nops();
    }

    let function_table = if conf.backtrace {
        cc.function_table()
    } else {
        HashMap::new()
    };

    let ctx = if conf.disassemble {
        Some(cc.clone())
    } else {
        None
    };
    let (mut vm, debug) = cc.finalize(VmConfig {
        backtrace: conf.backtrace,
    });

    if conf.disassemble {
        bc::dis::Disassembler::new(&vm.bytecode, ctx.unwrap()).disassemble();
    }

    if conf.dry {
        return Ok(());
    }

    if let Err(e) = vm.run() {
        println!(
            "{}",
            PgError::from_anomaly(e, &debug).render(input_source, input.as_bytes())
        );

        if conf.backtrace {
            let entry_point_pc = function_table
                .iter()
                .find(|(_, name)| name.as_str() == "entry")
                .map(|(pc, _)| *pc)
                .unwrap_or_default();
            vm.backtrace.insert(0, entry_point_pc);

            println!("at:");
            for (idx, trace_id) in vm.backtrace.iter().enumerate() {
                let Some(name) = function_table.get(trace_id) else {
                    panic!("Backtrace bug");
                };
                println!(" #{idx} {name}");
            }
        }

        std::process::exit(1);
    }

    trace!("[main] Executed bytecode");
    Ok(())
}

fn main() {
    if let Err(e) = entry() {
        println!("{e}");
        std::process::exit(1);
    }
}
