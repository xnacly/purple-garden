#![allow(dead_code, unused_variables)]

use crate::op::Op;

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

type Todo = ();

// TODO:
// - port pg cli to serde
// - port frontend (lexer, parser)
//      - port tokens
//      - port ast
// - port cc
// - port vm fully
// - port gc
// - implement very good errors
// - build peephole optimisation (constant folding, load elim) and x86 jit, can be enabled via -O1
// - build optimising x86 jit with ssa via -O2
// - allow for writing bytecode to disk
fn main() {
    let bytecode: Vec<Op> = vec![
        Op::LoadImm { dst: 0, value: 10 },
        Op::LoadImm { dst: 1, value: 32 },
        Op::Add {
            dst: 0,
            lhs: 0,
            rhs: 1,
        },
        Op::StoreLocal { slot: 0, src: 0 },
        Op::LoadLocal { slot: 0, dst: 2 },
        Op::LoadGlobal { dst: 0, idx: 1 },
        Op::Call {
            func: 1,
            args_start: 0,
            args_len: 2,
        },
        Op::Sys {
            ptr: |_, _| {},
            args_start: 0,
            args_len: 1,
        },
        Op::Ret,
    ];

    for (i, op) in bytecode.iter().enumerate() {
        println!("{:04} {:?}", i, op)
    }
}
