#![allow(dead_code, unused_variables)]
#![cfg_attr(feature = "nightly", feature(likely_unlikely))]

use std::fs;

use crate::{err::PgError, lex::Lexer, parser::Parser, vm::Value};

mod ast;
mod cc;
/// pretty print errors
mod err;
/// simple mark and sweep garbage collector, will be replaced by a manchester garbage collector in
/// the future
mod gc;
mod ir;
/// baseline just in time compilation for x86
mod jit;
mod lex;
/// purple garden bytecode virtual machine operations
mod op;
/// collection of ir and bytecode optimisation passes
mod opt;
mod parser;
/// register based virtual machine
mod vm;

#[derive(clap::Parser)]
#[command(about, version, long_about=None)]
struct Args {
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

    /// Readable bytecode
    #[arg(short, long)]
    disassemble: bool,
    /// Readable abstract syntax tree
    #[arg(short, long)]
    ast: bool,
    /// Readable immediate representation
    #[arg(short, long)]
    ir: bool,
    /// Readable used register print
    #[arg(short, long)]
    registers: bool,

    /// Limit the standard library to necessities
    #[arg(long)]
    no_std: bool,
    /// Skip importing of env variables
    #[arg(long)]
    no_env: bool,
    /// Disable garbage collection
    #[arg(long)]
    no_gc: bool,
    file: String,
}

fn main() {
    let args = <Args as clap::Parser>::parse();
    let input = fs::read(args.file).expect("Failed to read from file");
    let lexer = Lexer::new(&input);
    let ast = match Parser::new(lexer).parse() {
        Ok(a) => a,
        Err(e) => {
            e.render();
            std::process::exit(1);
        }
    };

    if args.ast {
        print!(
            "{}",
            ast.iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // TODO: add ir creation pass here
    if args.ir {
        println!("no ir implementation yet");
    }

    let mut cc = cc::Cc::new();
    if let Err(e) = cc.compile(&ast) {
        e.render();
        std::process::exit(1);
    }

    if args.opt > 1 {
        opt::bc(&mut cc.buf);
    }

    if args.disassemble {
        cc.dis();
    }

    let mut vm = cc.finalize();

    if let Err(e) = vm.run() {
        Into::<PgError>::into(e).render();
    }

    if args.registers {
        for i in 0..vm::REGISTER_COUNT {
            let val = &vm.registers[i];
            if let Value::UnDef = val {
                continue;
            }
            println!("r{i}={:?}", val);
        }
    }
}
