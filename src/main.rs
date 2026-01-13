#![allow(dead_code, unused_variables)]
#![cfg_attr(feature = "nightly", feature(likely_unlikely))]

use std::fs;

use crate::{err::PgError, lex::Lexer, parser::Parser};

mod ast;
mod cc;
/// pretty print errors
mod err;
/// simple mark and sweep garbage collector, will be replaced by a manchester garbage collector in
/// the future
mod gc;
mod lex;
/// purple garden bytecode virtual machine operations
mod op;
mod parser;
/// register based virtual machine
mod vm;

#[derive(clap::Parser)]
#[command(about, version, long_about=None)]
struct Args {
    /// Can be 0 for none, 1 for IR based optimisations like constant
    /// folding, rewrites, etc. And can be 2 for optimising ssa passes and JIT
    #[arg(short = 'O')]
    opt: usize,
    /// readable bytecode representation with labels, globals and comments
    #[arg(short, long)]
    disassemble: bool,
    /// limit the standard library to necessities
    #[arg(long)]
    no_std: bool,
    /// skip importing of env variables
    #[arg(long)]
    no_env: bool,
    /// disable garbage collection
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

    // TODO: add ir creation pass here

    let mut cc = cc::Cc::new();
    if let Err(e) = cc.compile(&ast) {
        e.render();
        std::process::exit(1);
    }
    cc.dis();

    let mut vm = cc.finalize();
    if let Err(e) = vm.run() {
        Into::<PgError>::into(e).render();
    }
}
