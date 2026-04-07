use purple_garden::{
    bc, config,
    err::PgError,
    help, ir,
    lex::Lexer,
    opt,
    parser::Parser,
    std::{self as pstd, Pkg},
    trace,
};
use std::{collections::HashMap, fs};

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

fn main() {
    let args = <config::Config as clap::Parser>::parse();
    match args.version {
        1 => {
            println!(
                "purple-garden version {} by xnacly and contributors",
                env!("CARGO_PKG_VERSION")
            );
            std::process::exit(0);
        }
        2 => {
            println!(
                "purple-garden version {} by xnacly and contributors",
                env!("CARGO_PKG_VERSION")
            );
            println!("{}", BUILD_INFO.replace(";", "\n"));
            let exe = std::env::current_exe().unwrap();
            println!("from: {}", exe.display());
            std::process::exit(0);
        }
        _ => {}
    }
    if let Some(ref cmd) = args.command {
        match &cmd {
            config::Command::Intro { topic } => {
                println!(
                    "{}",
                    help::print_help_by_topic(topic.as_ref().map(|x| x.as_str()))
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
                    for pkg in purple_garden::std::STD {
                        print_pkg(pkg)
                    }
                    std::process::exit(0);
                };

                let (path, method) = match pkg_or_function.split_once(".") {
                    Some((path, method)) => (path, Some(method)),
                    None => (pkg_or_function.as_str(), None),
                };

                let pkg = pstd::resolve_pkg(path).unwrap_or_else(|| {
                    eprintln!("query {} couldnt be resolved to anything", path);
                    std::process::exit(1);
                });

                if let Some(method) = method {
                    println!(
                        "{}",
                        pkg.fns
                            .iter()
                            .find(|f| f.name == method)
                            .unwrap_or_else(|| {
                                eprintln!("function {}.{} not found", pkg.name, method);
                                std::process::exit(1);
                            })
                    );
                } else {
                    println!("{}", pkg);
                }

                std::process::exit(0);
            }
        }
    }
    let input = match args.run {
        Some(ref i) => i.as_bytes().to_vec(),
        None => fs::read(args.target.clone().expect("No file or `-r` specified"))
            .expect("Failed to read from file")
            .to_vec(),
    };

    let lexer = Lexer::new(&input);
    let ast = match Parser::new(lexer).and_then(|n| n.parse()) {
        Ok(a) => a,
        Err(e) => {
            let lines = str::from_utf8(&input)
                .unwrap()
                .lines()
                .collect::<Vec<&str>>();
            e.render(&lines);
            std::process::exit(1);
        }
    };

    trace!("Tokenisation and Parsing done");

    if args.ast {
        print!(
            "{}",
            ast.iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join("")
        );
    }

    let lower = ir::lower::Lower::new();
    let mut ir = match lower.ir_from(&ast) {
        Ok(ir) => ir,
        Err(e) => {
            let lines = str::from_utf8(&input)
                .unwrap()
                .lines()
                .collect::<Vec<&str>>();
            e.render(&lines);
            std::process::exit(1);
        }
    };

    trace!("Lowered AST to IR");

    if args.opt >= 1 {
        opt::ir(&mut ir);
    }

    if args.ir {
        for func in ir.iter() {
            println!("{func}");
        }
    }

    let mut cc = bc::Cc::new();
    if let Err(e) = cc.compile(&ir) {
        let lines = str::from_utf8(&input)
            .unwrap()
            .lines()
            .collect::<Vec<&str>>();
        e.render(&lines);
        std::process::exit(1);
    };

    trace!("Lowered IR to bytecode");

    if args.opt >= 1 {
        opt::bc(&mut cc.buf);
    }

    let function_table = if args.backtrace {
        cc.function_table()
    } else {
        HashMap::new()
    };

    let ctx = if args.disassemble {
        Some(cc.clone())
    } else {
        None
    };
    let mut vm = cc.finalize(&args);

    if args.disassemble {
        bc::dis::Disassembler::new(&vm.bytecode, ctx.unwrap()).disassemble();
    }

    if args.dry {
        return;
    }

    if let Err(e) = vm.run() {
        let lines = str::from_utf8(&input)
            .unwrap()
            .lines()
            .collect::<Vec<&str>>();
        Into::<PgError>::into(e).render(&lines);

        if args.backtrace {
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
    }

    trace!("Executed bytecode");
}
