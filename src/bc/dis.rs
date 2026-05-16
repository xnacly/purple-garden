use std::collections::HashMap;

use crate::{
    bc,
    ir::{Const, Id},
    std as pstd,
    vm::{BuiltinFn, op::Op},
};

pub struct Disassembler<'dis> {
    bc: &'dis [Op],
    cc: bc::Cc<'dis>,
}

impl<'dis> Disassembler<'dis> {
    pub fn new(bc: &'dis [Op], cc: bc::Cc<'dis>) -> Self {
        Self { bc, cc }
    }

    /// maps the pointer to any stdlib function to its <pkg>.<name>
    pub fn build_fn_map() -> HashMap<BuiltinFn, String> {
        fn walk(
            pkgs: &'static [pstd::Pkg],
            path: &mut Vec<&'static str>,
            out: &mut HashMap<BuiltinFn, String>,
        ) {
            for pkg in pkgs {
                path.push(pkg.name);

                // register all functions in this package
                for f in pkg.fns {
                    let mut full = String::new();

                    for segment in path.iter() {
                        full.push_str(segment);
                        full.push('.');
                    }

                    full.push_str(f.name);

                    out.insert(f.ptr, full);
                }

                // recurse into subpackages
                walk(pkg.pkgs, path, out);

                path.pop();
            }
        }

        let mut map = HashMap::new();
        walk(pstd::STD, &mut Vec::new(), &mut map);
        map
    }

    pub fn disassemble(&self) {
        let funcs_by_pc: HashMap<u32, &bc::BcFunc> = self
            .cc
            .functions
            .values()
            .map(|f| (f.pc as u32, f))
            .collect();

        let globals = self.cc.globals.clone().into_vec();
        let strings = self.cc.strings.clone().into_vec();
        let std_fns = self.cc.std_fns.clone().into_vec();
        let std_mapping = Self::build_fn_map();

        if !globals.is_empty() {
            println!("globals:");
            for (i, g) in globals.iter().enumerate() {
                println!("  {:04}:    {}", i, g)
            }
        }

        if !strings.is_empty() {
            println!("strs:");
            for (i, s) in strings.iter().enumerate() {
                println!("  {:04}:    \"{}\"", i, s)
            }
        }

        let mut cur_func = self.cc.functions.get(&Id(0)).unwrap();
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
                        let val_str = globals[*idx as usize];

                        // only ints bigger than i32::MAX are interned as integers, all others are
                        // inlined into load_imm. Thus all i < i32::MAX are indexes into the string
                        // constant pool.
                        if let Const::Int(idx) = val_str
                            && idx < i32::MAX as i64
                        {
                            format!(
                                "load_global r{dst}, {idx} \t; = \"{}\"",
                                strings[idx as usize]
                            )
                        } else {
                            format!("load_global r{dst}, {idx} \t; = {}", val_str)
                        }
                    }
                    Op::Jmp { target } => {
                        format!(
                            "jmp {target} <{}+0x{:0x}>",
                            cur_func.name,
                            *target as usize - cur_func.pc
                        )
                    }
                    Op::Tail { func } => {
                        format!("tail {func} <{}>", funcs_by_pc.get(func).unwrap().name)
                    }
                    Op::JmpT { cond, target } => format!(
                        "jmpt r{cond}, {target} <{}+0x{:0x}>",
                        cur_func.name,
                        *target as usize - cur_func.pc
                    ),
                    Op::Call { func } =>
                        format!("call {func} <{}>", funcs_by_pc.get(func).unwrap().name),
                    Op::Sys { idx } => format!(
                        "sys {idx} <{}> \t; @ 0x{:x}",
                        std_mapping.get(&std_fns[*idx as usize]).unwrap(),
                        std_fns[*idx as usize] as usize,
                    ),
                    Op::Push { src } => format!("push r{src}"),
                    Op::Pop { dst } => format!("pop r{dst}"),
                    Op::Ret => "ret".into(),
                    Op::CastToBool { dst, src } => {
                        format!("cast_to_bool r{dst}, r{src}")
                    }
                    Op::CastToInt { dst, src } => {
                        format!("cast_to_int r{dst}, r{src}")
                    }
                    Op::CastToDouble { dst, src } => {
                        format!("cast_to_double r{dst}, r{src}")
                    }
                    Op::Nop => "nop".into(),
                }
            );
        }
    }
}
