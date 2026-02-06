use std::collections::HashMap;

use crate::{
    bc::{Cc, ctx::Func},
    ir::Const,
    vm::{self, op::Op},
};

impl Cc<'_> {
    pub fn function_table(&self) -> HashMap<usize, String> {
        self.ctx
            .functions
            .clone()
            .into_values()
            .map(|v| (v.pc, v.name.to_string()))
            .collect()
    }

    pub fn dis(&self) {
        let reverse_function_lookup_table: HashMap<usize, Func<'_>> = self
            .ctx
            .functions
            .clone()
            .into_values()
            .map(|v| (v.pc, v))
            .collect();

        let reverse_global_lookup_table: HashMap<_, _> = self
            .ctx
            .globals
            .clone()
            .into_iter()
            .map(|(k, v)| (v, k))
            .collect();

        println!("__entry: ");

        for (i, b) in self.buf.iter().enumerate() {
            if let Some(func) = reverse_function_lookup_table.get(&i) {
                println!(
                    "\n__{}: \t\t\t; 0x{:04X} args={};size={}",
                    func.name, func.pc, func.args, func.size
                );
            }

            println!(
                "\t{}",
                match b {
                    Op::Add { dst, lhs, rhs } => format!("add r{dst}, r{lhs}, r{rhs}"),
                    Op::Sub { dst, lhs, rhs } => format!("sub r{dst}, r{lhs}, r{rhs}"),
                    Op::Mul { dst, lhs, rhs } => format!("mul r{dst}, r{lhs}, r{rhs}"),
                    Op::Div { dst, lhs, rhs } => format!("div r{dst}, r{lhs}, r{rhs}"),
                    Op::Eq { dst, lhs, rhs } => format!("eq r{dst}, r{lhs}, r{rhs}"),
                    Op::Lt { dst, lhs, rhs } => format!("lt r{dst}, r{lhs}, r{rhs}"),
                    Op::Gt { dst, lhs, rhs } => format!("gt r{dst}, r{lhs}, r{rhs}"),
                    Op::Not { dst, src } => format!("not r{dst}, r{src}"),
                    Op::Mov { dst, src } => format!("mov r{dst}, r{src}"),
                    Op::LoadImm { dst, value } => format!("load_imm r{dst}, #{value}"),
                    Op::LoadGlobal { dst, idx } => format!("load_global r{dst}, {idx} \t; {:?}", {
                        let raw_global = reverse_global_lookup_table.get(&(*idx as usize));
                        <Const<'_> as Into<vm::Value>>::into(*raw_global.unwrap())
                    }),
                    Op::Jmp { target } => format!("jmp {target}"),
                    Op::JmpF { cond, target } => format!("jmpf r{cond}, {target}"),
                    Op::Call { func } => format!(
                        "call {}",
                        reverse_function_lookup_table
                            .get(&(*func as usize))
                            .unwrap()
                            .name
                    ),
                    Op::Sys { .. } => "sys <syscall_name_here>".to_string(),
                    Op::Ret => "ret\n".into(),
                    Op::Nop => "nop".into(),
                }
            );
        }
    }
}
