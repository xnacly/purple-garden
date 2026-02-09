#![cfg_attr(feature = "nightly", feature(likely_unlikely))]
#![allow(unused)]

use std::{collections::HashMap, fs};

#[cfg(feature = "trace")]
mod trace {
    use super::*;
    use std::sync::Once;
    use std::time::Instant;

    static START_ONCE: Once = Once::new();
    static mut START: Option<Instant> = None;

    pub fn start() -> Instant {
        unsafe {
            START_ONCE.call_once(|| {
                START = Some(Instant::now());
            });
            START.unwrap()
        }
    }

    #[macro_export]
    macro_rules! trace {
        // With values
        ($fmt:literal, $($value:expr),*) => {
            {
                let elapsed = $crate::trace::start().elapsed();
                println!("[{:?}] {}", elapsed, format!($fmt, $($value),*));
            }
        };
        // Without values
        ($fmt:literal) => {
            {
                let elapsed = $crate::trace::start().elapsed();
                println!("[{:?}] {}", elapsed, $fmt);
            }
        };
    }
}

#[cfg(not(feature = "trace"))]
mod trace {
    #[macro_export]
    macro_rules! trace {
        ($fmt:literal, $($value:expr),*) => {};
        ($fmt:literal) => {};
    }
}

use crate::{err::PgError, lex::Lexer, parser::Parser, vm::Value};

mod asm;
mod ast;
mod bc;
/// pretty print errors
mod err;
/// simple mark and sweep garbage collector, will be replaced by a manchester style garbage
/// collector in the future
mod gc;
mod ir;
/// baseline just in time compilation for x86 and aarch64
mod jit;
mod lex;
/// collection of ir and bytecode optimisation passes
mod opt;
mod parser;
/// register based virtual machine
mod vm;

#[derive(clap::Parser, Debug, Default)]
#[command(about, version, long_about=None)]
pub struct Args {
    /// Set optimisation level. Higher levels increase compile time. All levels preserve language
    /// semantics.
    ///
    /// 0: Baseline lowering with no optimisation passes.
    ///
    /// 1: Local IR and bytecode optimisations:
    ///    constant folding and propagation, arithmetic simplification,
    ///    peephole bytecode cleanup, redundant load elimination.
    ///
    /// 2: Global IR optimisations:
    ///    SSA construction, control-flow aware dead code elimination,
    ///    register lifetime minimisation, copy propagation.
    ///    Includes all -O1 optimisations.
    ///
    /// 3: Aggressive compile-time optimisations:
    ///    function inlining, guarded operator specialisation,
    ///    constant hoisting, aggressive register reuse.
    ///    Includes all -O2 optimisations.
    #[arg(short = 'O', default_value_t = 0)]
    opt: usize,

    /// Compile the target into native machine code and execute said code
    #[arg(short = 'N', long)]
    native: bool,

    /// Readable bytecode or machine code, depending on execution strategy
    #[arg(short = 'D', long)]
    disassemble: bool,
    /// Readable abstract syntax tree
    #[arg(short = 'A', long)]
    ast: bool,
    /// Readable immediate representation
    #[arg(short = 'I', long)]
    ir: bool,
    /// Readable used register print
    #[arg(short = 'R', long)]
    registers: bool,
    /// Generate backtraces for function calls
    ///
    /// Technically a brain child of my interview at apple in which we talked about ways of implementing
    /// backtraces for error display for javascript.
    #[arg(short = 'B', long)]
    backtrace: bool,

    /// Limit the standard library to necessities
    #[arg(long)]
    no_std: bool,
    /// Skip importing of env variables
    #[arg(long)]
    no_env: bool,
    /// Disable garbage collection
    #[arg(long)]
    no_gc: bool,
    /// Disable Just In Time compilation
    #[arg(long)]
    no_jit: bool,

    /// run a single string passed via this flag instead of a file
    #[arg(short)]
    run: Option<String>,

    target: Option<String>,
}

fn main() {
    let args = <Args as clap::Parser>::parse();
    let input = match args.run {
        Some(ref i) => i.as_bytes().to_vec(),
        None => fs::read(args.target.clone().expect("No file or `-r` specified"))
            .expect("Failed to read from file")
            .to_vec(),
    };
    let mut lexer = Lexer::new(&input);
    let ast = match Parser::new(&mut lexer).and_then(|n| n.parse()) {
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
    let ir = match lower.ir_from(&ast) {
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

    if args.disassemble {
        cc.dis();
    }

    let mut function_table = if args.backtrace {
        cc.function_table()
    } else {
        HashMap::new()
    };

    let mut vm = cc.finalize(&args);

    if let Err(e) = vm.run() {
        let lines = str::from_utf8(&input)
            .unwrap()
            .lines()
            .collect::<Vec<&str>>();
        Into::<PgError>::into(e).render(&lines);
        if args.backtrace {
            function_table.insert(0, "entry".into());
            println!("at:");
            vm.backtrace.insert(0, 0);
            for (idx, trace_id) in vm.backtrace.iter().rev().enumerate() {
                let Some(name) = function_table.get(&trace_id) else {
                    panic!("Backtrace bug");
                };
                println!(" #{idx} {name}");
            }
        }
    }

    trace!("Executed bytecode");

    if args.registers {
        for i in 0..vm::REGISTER_COUNT {
            let val = &vm.r[i];
            if val.tag() == Value::UNDEF {
                continue;
            }
            println!("[r{i}]={}", val);
        }
    }
}
