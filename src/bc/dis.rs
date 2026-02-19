use std::collections::HashMap;

use crate::{
    bc::{
        Cc,
        ctx::{self, Func},
    },
    ir::{self, Const},
    vm::{self, op::Op},
};

pub struct Disassembler<'dis> {
    bc: &'dis [Op],
    ctx: ctx::Context<'dis>,
}

impl<'dis> Disassembler<'dis> {
    pub fn new(bc: &'dis [Op], ctx: ctx::Context<'dis>) -> Self {
        Self { bc, ctx }
    }

    pub fn disassemble(&self) {
        let funcs_by_pc: HashMap<u32, &Func> = self
            .ctx
            .functions
            .values()
            .map(|f| (f.pc as u32, f))
            .collect();
        let globals_by_idx: HashMap<u32, &Const> = self
            .ctx
            .globals
            .iter()
            .map(|(c, idx)| (*idx as u32, c))
            .collect();
        for (pc, instr) in self.bc.iter().enumerate() {
            if let Some(func) = funcs_by_pc.get(&(pc as u32)) {
                println!("{}:", func.name);
            }

            println!(
                "\t{}",
                self.format_instr(instr, &globals_by_idx, &funcs_by_pc)
            );
        }
    }

    fn format_instr(
        &self,
        instr: &Op,
        globals_by_idx: &HashMap<u32, &Const>,
        funcs_by_pc: &HashMap<u32, &Func>,
    ) -> String {
        match instr {
            Op::IAdd { dst, lhs, rhs } => format!("add r{dst}, r{lhs}, r{rhs}"),
            Op::ISub { dst, lhs, rhs } => format!("sub r{dst}, r{lhs}, r{rhs}"),
            Op::IMul { dst, lhs, rhs } => format!("mul r{dst}, r{lhs}, r{rhs}"),
            Op::IDiv { dst, lhs, rhs } => format!("div r{dst}, r{lhs}, r{rhs}"),
            Op::Eq { dst, lhs, rhs } => format!("eq r{dst}, r{lhs}, r{rhs}"),
            Op::Lt { dst, lhs, rhs } => format!("lt r{dst}, r{lhs}, r{rhs}"),
            Op::Gt { dst, lhs, rhs } => format!("gt r{dst}, r{lhs}, r{rhs}"),
            Op::Mov { dst, src } => format!("mov r{dst}, r{src}"),
            Op::LoadI { dst, value } => format!("load_imm r{dst}, #{value}"),
            Op::LoadG { dst, idx } => {
                let val_str = globals_by_idx
                    .get(idx)
                    .map(|v| format!("{:?}", v))
                    .unwrap_or("<unknown>".to_string());
                format!("load_global r{dst}, {idx} \t; {}", val_str)
            }
            Op::Jmp { target } => format!("jmp {target}",),
            Op::JmpF { cond, target } => format!("jmpf r{cond}, {target}",),
            Op::Call { func } => format!("call {}", funcs_by_pc.get(func).unwrap().name),
            Op::Sys { .. } => "sys <syscall_name_here>".to_string(),
            Op::Push { src } => format!("push {src}"),
            Op::Pop { dst } => format!("pop {dst}"),
            Op::Ret => "ret".into(),
            Op::Nop => "nop".into(),
        }
    }
}
