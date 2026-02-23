use std::collections::HashMap;

use crate::{
    bc::{
        Cc,
        ctx::{self, Func},
    },
    ir::{self, Const, Id},
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

        let mut cur_func = self.ctx.functions.get(&Id(0)).unwrap();
        for (pc, instr) in self.bc.iter().enumerate() {
            if let Some(func) = funcs_by_pc.get(&(pc as u32)) {
                cur_func = func;
                println!("\n{:08x} <{}>:", pc, func.name);
            }

            println!(
                "  {:04x}:    {}",
                pc,
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
                    Op::Jmp { target } => {
                        format!(
                            "jmp {target} <{}+0x{:0X}>",
                            cur_func.name,
                            *target as usize - cur_func.pc
                        )
                    }
                    Op::JmpF { cond, target } => format!(
                        "jmpf r{cond}, {target} <{}+0x{:0X}>",
                        cur_func.name,
                        pc - cur_func.pc
                    ),
                    Op::Call { func } =>
                        format!("call {func} <{}>", funcs_by_pc.get(func).unwrap().name),
                    Op::Sys { .. } => "sys <syscall_name_here>".to_string(),
                    Op::Push { src } => format!("push {src}"),
                    Op::Pop { dst } => format!("pop {dst}"),
                    Op::Ret => "ret".into(),
                    Op::CastToBool { dst, src } => {
                        format!("cast_to_bool {dst}, {src}")
                    }
                    Op::CastToInt { dst, src } => {
                        format!("cast_to_int {dst}, {src}")
                    }
                    Op::CastToDouble { dst, src } => {
                        format!("cast_to_double {dst}, {src}")
                    }
                    Op::Nop => "nop".into(),
                }
            );
        }
    }
}
