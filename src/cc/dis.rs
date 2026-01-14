use crate::{cc::Cc, op::Op};

impl Cc<'_> {
    // TODO: disconnect this from the compiler so finalize doesnt annoy us
    pub fn dis(&self) {
        println!("__entry: ");
        for (i, b) in self.buf.iter().enumerate() {
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
                    Op::LoadGlobal { dst, idx } => format!(
                        "load_global r{dst}, {idx} ; {:?}",
                        self.ctx.globals_vec.get(*idx as usize)
                    ),
                    Op::Jmp { target } => format!("jmp {target}"),
                    Op::JmpF { cond, target } => format!("jmpf r{cond}, {target}"),
                    Op::Call {
                        func,
                        args_start,
                        args_len,
                    } => format!("call {func}, {args_start}, {args_len}"),
                    Op::Sys {
                        ptr,
                        args_start,
                        args_len,
                    } => format!("sys {:p}, {args_start}, {args_len}", ptr),
                    Op::Ret => "ret".into(),
                    Op::Nop => "nop".into(),
                }
            );
        }
    }
}
