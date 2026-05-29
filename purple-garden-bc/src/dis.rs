use std::collections::HashMap;
use std::io::IsTerminal;

use purple_garden_ir::Id;
use purple_garden_runtime::{BuiltinFn, op::Op};
use purple_garden_std as pstd;

// ANSI SGR codes, applied only when stdout is a tty and NO_COLOR is unset.
const ADDR: &str = "90"; // gray   - pc / addresses
const MNEM: &str = "36"; // cyan   - data mnemonics
const FLOW: &str = "1;35"; // magenta - control-flow mnemonics
const REG: &str = "32"; // green  - registers
const IMM: &str = "33"; // yellow - immediates
const LABEL: &str = "94"; // blue   - jump targets / symbols
const COMMENT: &str = "90"; // gray   - source/value annotations
const FUNC: &str = "1;33"; // bold yellow - function headers
const BLOCK: &str = "1;94"; // bold blue   - basic-block labels
const SECTION: &str = "1"; // bold        - section headers

fn paint(on: bool, code: &str, s: &str) -> String {
    if on {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

/// classifies a single operand token (`r3`, `#5`, `000d`, ...) and wraps it.
fn paint_token(on: bool, tok: &str) -> String {
    if tok.starts_with('r') && tok.len() > 1 && tok[1..].bytes().all(|b| b.is_ascii_digit()) {
        paint(on, REG, tok)
    } else if tok.starts_with('#') {
        paint(on, IMM, tok)
    } else if tok.bytes().all(|b| b.is_ascii_hexdigit()) {
        paint(on, LABEL, tok)
    } else {
        tok.to_string()
    }
}

/// colorizes a rendered instruction string without changing its visible width:
/// the mnemonic, then each operand token, with `<...>` symbols kept whole.
fn colorize_instr(on: bool, s: &str) -> String {
    if !on {
        return s.to_string();
    }
    let (mnem, rest) = s.split_once(' ').unwrap_or((s, ""));
    let flow = matches!(
        mnem,
        "jmp" | "jmpt" | "jmpf" | "call" | "tail" | "ret" | "sys"
    );
    let mut out = paint(on, if flow { FLOW } else { MNEM }, mnem);
    if !rest.is_empty() {
        out.push(' ');
    }

    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '<' {
            let end = rest[i..].find('>').map_or(rest.len(), |j| i + j + 1);
            out.push_str(&paint(on, LABEL, &rest[i..end]));
            i = end;
        } else if c.is_ascii_alphanumeric() || matches!(c, '#' | '-' | '_' | '.') {
            let start = i;
            while i < bytes.len() {
                let d = bytes[i] as char;
                if d.is_ascii_alphanumeric() || matches!(d, '#' | '-' | '_' | '.') {
                    i += 1;
                } else {
                    break;
                }
            }
            out.push_str(&paint_token(on, &rest[start..i]));
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

pub struct Disassembler<'dis> {
    bc: &'dis [Op],
    cc: crate::Cc<'dis>,
    source: Option<&'dis [u8]>,
}

impl<'dis> Disassembler<'dis> {
    #[must_use]
    pub fn new(bc: &'dis [Op], cc: crate::Cc<'dis>) -> Self {
        Self {
            bc,
            cc,
            source: None,
        }
    }

    #[must_use]
    pub fn with_source(mut self, source: &'dis [u8]) -> Self {
        self.source = Some(source);
        self
    }

    /// maps the pointer to any stdlib function to its <pkg>.<name>
    #[must_use]
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
        let funcs_by_pc: HashMap<u32, &crate::CcFunc> = self
            .cc
            .functions
            .values()
            .filter_map(|f| Some((f.pc()? as u32, f)))
            .collect();
        let mut funcs_sorted: Vec<(usize, &crate::CcFunc)> = funcs_by_pc
            .iter()
            .map(|(&pc, &func)| (pc as usize, func))
            .collect();
        funcs_sorted.sort_by_key(|(pc, _)| *pc);

        let containing_func = |pc: usize| -> Option<&crate::CcFunc> {
            funcs_sorted
                .iter()
                .rev()
                .find(|(func_pc, _)| *func_pc <= pc)
                .map(|(_, func)| *func)
        };

        let mut block_labels: HashMap<usize, String> = HashMap::new();
        for instr in self.bc {
            match instr {
                Op::Jmp { target } | Op::JmpT { target, .. } | Op::JmpF { target, .. } => {
                    let pc = *target as usize;
                    if funcs_by_pc.contains_key(&(*target as u32)) {
                        continue;
                    }
                    let name = containing_func(pc)
                        .map(crate::CcFunc::name)
                        .unwrap_or("bytecode");
                    block_labels
                        .entry(pc)
                        .or_insert_with(|| format!("{name}.bb_{pc:04x}"));
                }
                _ => {}
            }
        }

        let native_names: HashMap<u16, String> = self
            .cc
            .functions
            .values()
            .filter_map(|f| match f {
                crate::CcFunc::Native { idx, name } => Some((*idx, format!("jit_{name}"))),
                crate::CcFunc::Bc { .. } => None,
            })
            .collect();

        let globals = self.cc.globals.to_vec();
        let (str_data, str_spans) = self.cc.strings.into_arena();
        let std_fns = self.cc.std_fns.to_vec();
        let std_mapping = Self::build_fn_map();
        let target_label = |target: u16, cur_func: &crate::CcFunc| -> String {
            if let Some(label) = block_labels.get(&(target as usize)) {
                return label.clone();
            }

            if let Some(func) = funcs_by_pc.get(&(target as u32)) {
                return func.name().to_string();
            }

            let base = cur_func.pc().unwrap_or_default();
            if target as usize >= base {
                format!("{}+0x{:0x}", cur_func.name(), target as usize - base)
            } else {
                format!("{}-0x{:0x}", cur_func.name(), base - target as usize)
            }
        };

        let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();

        if !globals.is_empty() {
            println!("{}", paint(color, SECTION, "globals:"));
            for (i, g) in globals.iter().enumerate() {
                println!("  {}:    {}", paint(color, ADDR, &format!("{i:04}")), paint(color, IMM, &g.to_string()));
            }
        }

        if !str_spans.is_empty() {
            println!("{}", paint(color, SECTION, "strs:"));
            for (i, &(off, len)) in str_spans.iter().enumerate() {
                let s = &str_data[off as usize..off as usize + len as usize];
                println!(
                    "  {}:    {}",
                    paint(color, ADDR, &format!("{i:04}")),
                    paint(color, LABEL, &format!("\"{s}\""))
                );
            }
        }

        let render = |instr: &Op, cur_func: &crate::CcFunc| -> String {
            match instr {
                Op::IAdd { dst, lhs, rhs } => format!("iadd r{dst}, r{lhs}, r{rhs}"),
                Op::IAddI { dst, lhs, imm } => format!("iadd_imm r{dst}, r{lhs}, #{imm}"),
                Op::ISub { dst, lhs, rhs } => format!("isub r{dst}, r{lhs}, r{rhs}"),
                Op::ISubI { dst, lhs, imm } => format!("isub_imm r{dst}, r{lhs}, #{imm}"),
                Op::IMul { dst, lhs, rhs } => format!("imul r{dst}, r{lhs}, r{rhs}"),
                Op::IMulI { dst, lhs, imm } => format!("imul_imm r{dst}, r{lhs}, #{imm}"),
                Op::IDiv { dst, lhs, rhs } => format!("idiv r{dst}, r{lhs}, r{rhs}"),
                Op::IDivI { dst, lhs, imm } => format!("idiv_imm r{dst}, r{lhs}, #{imm}"),
                Op::IEq { dst, lhs, rhs } => format!("ieq r{dst}, r{lhs}, r{rhs}"),
                Op::IEqI { dst, lhs, imm } => format!("ieq_imm r{dst}, r{lhs}, #{imm}"),
                Op::ILt { dst, lhs, rhs } => format!("ilt r{dst}, r{lhs}, r{rhs}"),
                Op::ILtI { dst, lhs, imm } => format!("ilt_imm r{dst}, r{lhs}, #{imm}"),
                Op::IGt { dst, lhs, rhs } => format!("igt r{dst}, r{lhs}, r{rhs}"),
                Op::IGtI { dst, lhs, imm } => format!("igt_imm r{dst}, r{lhs}, #{imm}"),
                Op::DAdd { dst, lhs, rhs } => format!("dadd r{dst}, r{lhs}, r{rhs}"),
                Op::DSub { dst, lhs, rhs } => format!("dsub r{dst}, r{lhs}, r{rhs}"),
                Op::DMul { dst, lhs, rhs } => format!("dmul r{dst}, r{lhs}, r{rhs}"),
                Op::DDiv { dst, lhs, rhs } => format!("ddiv r{dst}, r{lhs}, r{rhs}"),
                Op::DLt { dst, lhs, rhs } => format!("dlt r{dst}, r{lhs}, r{rhs}"),
                Op::DGt { dst, lhs, rhs } => format!("dgt r{dst}, r{lhs}, r{rhs}"),
                Op::BEq { dst, lhs, rhs } => format!("beq r{dst}, r{lhs}, r{rhs}"),
                Op::Mov { dst, src } => format!("mov r{dst}, r{src}"),
                Op::LoadI { dst, value } => format!("load_imm r{dst}, #{value}"),
                Op::LoadG { dst, idx } => format!("load_global r{dst}, {idx}"),
                Op::Jmp { target } => {
                    format!("jmp {target:04x} <{}>", target_label(*target, cur_func))
                }
                Op::Tail { func } => {
                    format!(
                        "tail {func:04x} <{}>",
                        funcs_by_pc.get(func).unwrap().name()
                    )
                }
                Op::JmpT { cond, target } => format!(
                    "jmpt r{cond}, {target:04x} <{}>",
                    target_label(*target, cur_func)
                ),
                Op::JmpF { cond, target } => format!(
                    "jmpf r{cond}, {target:04x} <{}>",
                    target_label(*target, cur_func)
                ),
                Op::Call { func } => format!(
                    "call {func:04x} <{}>",
                    funcs_by_pc.get(func).unwrap().name()
                ),
                Op::Sys { idx } => format!(
                    "sys {idx} <{}>",
                    native_names
                        .get(idx)
                        .map(String::as_str)
                        .or_else(|| std_mapping.get(&std_fns[*idx as usize]).map(String::as_str))
                        .unwrap_or("???"),
                ),
                Op::Push { src } => format!("push r{src}"),
                Op::Push2 { a, b } => format!("push2 r{a}, r{b}"),
                Op::Push3 { a, b, c } => format!("push3 r{a}, r{b}, r{c}"),
                Op::Pop { dst } => format!("pop r{dst}"),
                Op::Pop2 { a, b } => format!("pop2 r{a}, r{b}"),
                Op::Pop3 { a, b, c } => format!("pop3 r{a}, r{b}, r{c}"),
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
        };

        let entry = self.cc.functions.get(&Id(0)).unwrap();
        let mut cur_func = entry;
        let rendered: Vec<String> = self
            .bc
            .iter()
            .enumerate()
            .map(|(pc, instr)| {
                if let Some(func) = funcs_by_pc.get(&(pc as u32)) {
                    cur_func = func;
                }
                render(instr, cur_func)
            })
            .collect();
        let width = rendered.iter().map(String::len).max().unwrap_or(0);

        let mut last_source_line = None;
        let mut func_def_line = None;
        for (pc, instr) in rendered.iter().enumerate() {
            let is_entry = funcs_by_pc.contains_key(&(pc as u32));
            if let Some(func) = funcs_by_pc.get(&(pc as u32)) {
                println!(
                    "\n{:08x} {}:",
                    pc,
                    paint(color, FUNC, &format!("<{}>", func.name()))
                );
                last_source_line = None;
                func_def_line = self.source_line(pc).map(|(n, _)| n);
            }
            if let Some(label) = block_labels.get(&pc) {
                println!("{pc:08x} {}:", paint(color, BLOCK, &format!("<{label}>")));
            }

            let addr = paint(color, ADDR, &format!("{pc:04x}:"));
            let body = colorize_instr(color, instr);
            // the return-value move inherits the function's definition span; the
            // prologue already shows that line, so don't re-attach it mid-body.
            if let Some((line_no, line)) = self.source_line(pc)
                && last_source_line != Some((line_no, line))
                && !(!is_entry && Some(line_no) == func_def_line)
            {
                let pad = " ".repeat(width.saturating_sub(instr.len()));
                let note = paint(color, COMMENT, &format!("; {line_no}: {line}"));
                println!("  {addr}    {body}{pad} {note}");
                last_source_line = Some((line_no, line));
            } else {
                println!("  {addr}    {body}");
            }
        }
    }

    fn source_line(&self, pc: usize) -> Option<(usize, &'dis str)> {
        let source = self.source?;
        let span = *self.cc.pc_to_span.get(pc)?;
        let offset = span as usize;
        if offset >= source.len() {
            return None;
        }

        let line_start = source[..offset]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |idx| idx + 1);
        let line_end = source[offset..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(source.len(), |idx| offset + idx);
        let line_no = source[..line_start].iter().filter(|&&b| b == b'\n').count() + 1;

        let line = str::from_utf8(&source[line_start..line_end]).ok()?.trim();
        if line.is_empty() {
            return None;
        }

        Some((line_no, line))
    }
}
