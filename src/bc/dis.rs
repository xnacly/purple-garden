use std::collections::HashMap;

use crate::{
    bc::ctx::{self, Func},
    ir::{Const, Id},
    vm::op::Op,
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

        if !self.ctx.globals.is_empty() {
            println!("globals:");
            for g in &self.ctx.globals {
                println!("  {:04}:    {}", g.1, g.0)
            }
        }

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
                    Op::IAdd { dst, lhs, rhs } => format!("iadd r{dst}, r{lhs}, r{rhs}"),
                    Op::ISub { dst, lhs, rhs } => format!("isub r{dst}, r{lhs}, r{rhs}"),
                    Op::IMul { dst, lhs, rhs } => format!("imul r{dst}, r{lhs}, r{rhs}"),
                    Op::IDiv { dst, lhs, rhs } => format!("idiv r{dst}, r{lhs}, r{rhs}"),
                    Op::IEq { dst, lhs, rhs } => format!("ieq r{dst}, r{lhs}, r{rhs}"),
                    Op::ILt { dst, lhs, rhs } => format!("ilt r{dst}, r{lhs}, r{rhs}"),
                    Op::IGt { dst, lhs, rhs } => format!("igt r{dst}, r{lhs}, r{rhs}"),
                    Op::DAdd { dst, lhs, rhs } => format!("dadd r{dst}, r{lhs}, r{rhs}"),
                    Op::DSub { dst, lhs, rhs } => format!("dsub r{dst}, r{lhs}, r{rhs}"),
                    Op::DMul { dst, lhs, rhs } => format!("dmul r{dst}, r{lhs}, r{rhs}"),
                    Op::DDiv { dst, lhs, rhs } => format!("ddiv r{dst}, r{lhs}, r{rhs}"),
                    Op::DLt { dst, lhs, rhs } => format!("dlt r{dst}, r{lhs}, r{rhs}"),
                    Op::DGt { dst, lhs, rhs } => format!("dgt r{dst}, r{lhs}, r{rhs}"),
                    Op::BEq { dst, lhs, rhs } => format!("beq r{dst}, r{lhs}, r{rhs}"),
                    Op::Mov { dst, src } => format!("mov r{dst}, r{src}"),
                    Op::LoadI { dst, value } => format!("load_imm r{dst}, #{value}"),
                    Op::LoadG { dst, idx } => {
                        let val_str = globals_by_idx
                            .get(idx)
                            .map(|v| format!("{}", v))
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
                        *target as usize - cur_func.pc
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
