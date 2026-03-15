use purple_garden::{
    bc, config, err::PgError, ir, lex::Lexer, opt, parser::Parser, std::Pkg, trace,
};
use std::{collections::HashMap, fs};

fn main() {
    let args = <config::Config as clap::Parser>::parse();
    if let Some(cmd) = args.command {
        match cmd {
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

                let path: Vec<_> = path.split("/").collect();
                if path.is_empty() {
                    eprintln!("No path segment provided");
                    std::process::exit(1);
                }

                let mut pkg = purple_garden::std::STD
                    .iter()
                    .find(|p| p.name == path[0])
                    .unwrap_or_else(|| {
                        eprintln!("No matching root package found");
                        std::process::exit(1);
                    });

                for segment in &path[1..] {
                    pkg = pkg
                        .pkgs
                        .iter()
                        .find(|p| p.name == *segment)
                        .unwrap_or_else(|| {
                            eprintln!("pkg `{}` not found in `{}`", segment, pkg.name);
                            std::process::exit(1);
                        });
                }

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

    let mut cc = bc::Cc::default();
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
        Some(cc.ctx.clone())
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

    // Trust me bro r0 is a Str, please please please.
    //
    // terminated by signal SIGSEGV
    // println!("{}", vm.r[0].dbg(&vm, ir::ptype::Type::Str));

    trace!("Executed bytecode");
}
