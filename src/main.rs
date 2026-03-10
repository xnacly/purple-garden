use purple_garden::{bc, config, err::PgError, ir, lex::Lexer, opt, parser::Parser, trace};
use std::{collections::HashMap, fs};

fn main() {
    let args = <config::Config as clap::Parser>::parse();
    if let Some(cmd) = args.command {
        match cmd {
            config::Command::Doc { pkg_or_function } => {
                todo!("doc${pkg_or_function}");
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
