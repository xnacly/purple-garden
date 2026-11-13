use purple_garden_ir::Id;
use purple_garden_runtime::{
    BuiltinFn,
    op::{LookupTable, Op, SwitchKind, SwitchTable},
};
use purple_garden_shared::ansi::{Class, Highlights, paint, paint_highlights};
use purple_garden_std as pstd;
use std::collections::HashMap;

/// The class of a single operand token (`r3`, `#5`, `000d`, ...).
fn token_class(tok: &str) -> Option<Class> {
    if tok.starts_with('r') && tok.len() > 1 && tok[1..].bytes().all(|b| b.is_ascii_digit()) {
        Some(Class::Reg)
    } else if tok.starts_with('#') {
        Some(Class::Imm)
    } else if tok.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(Class::Label)
    } else {
        None
    }
}

/// Highlights of a rendered instruction: the mnemonic, then each operand token, with `<...>`
/// symbols kept whole.
#[must_use]
pub fn highlight_instr(s: &str) -> Highlights {
    let mnem_end = s.find(' ').unwrap_or(s.len());
    let flow = matches!(
        &s[..mnem_end],
        "jmp" | "jmpt" | "jmpf" | "call" | "tail" | "ret" | "sys"
    );
    let mut out = vec![(0..mnem_end, if flow { Class::Flow } else { Class::Mnem })];

    let bytes = s.as_bytes();
    let word = |c: char| c.is_ascii_alphanumeric() || matches!(c, '#' | '-' | '_' | '.');
    let mut i = mnem_end;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '<' {
            let end = s[i..].find('>').map_or(s.len(), |j| i + j + 1);
            out.push((i..end, Class::Label));
            i = end;
        } else if word(c) {
            let start = i;
            while i < bytes.len() && word(bytes[i] as char) {
                i += 1;
            }
            if let Some(class) = token_class(&s[start..i]) {
                out.push((start..i, class));
            }
        } else {
            i += 1;
        }
    }
    out
}

/// One line of the bytecode listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// The first op of a function is at `pc`.
    Func {
        pc: usize,
        name: String,
    },
    /// A jump target inside a function is at `pc`.
    Block {
        pc: usize,
        label: String,
    },
    Op {
        pc: usize,
        text: String,
    },
}

pub struct Disassembler<
    'dis,
    S: std::alloc::Allocator + Clone = std::alloc::Global,
    A: std::alloc::Allocator + Clone = std::alloc::Global,
> {
    bc: &'dis [Op],
    switch_tables: &'dis [SwitchTable],
    lookup_tables: &'dis [LookupTable],
    cc: crate::Cc<'dis, S, A>,
    source: Option<&'dis [u8]>,
}

impl<'dis, S: std::alloc::Allocator + Clone, A: std::alloc::Allocator + Clone>
    Disassembler<'dis, S, A>
{
    #[must_use]
    pub fn new(
        bc: &'dis [Op],
        switch_tables: &'dis [SwitchTable],
        lookup_tables: &'dis [LookupTable],
        cc: crate::Cc<'dis, S, A>,
    ) -> Self {
        Self {
            bc,
            switch_tables,
            lookup_tables,
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

    /// The constants in `globals`, by slot.
    #[must_use]
    pub fn globals(&self) -> Vec<String> {
        self.cc
            .globals
            .to_vec()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// The bytecode listing, function and block labels in front of their first op.
    #[must_use]
    pub fn lines(&self) -> Vec<Line> {
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
            let targets = match instr {
                Op::Jmp { target }
                | Op::JmpT { target, .. }
                | Op::JmpF { target, .. }
                | Op::JmpEqI { target, .. }
                | Op::JmpNeI { target, .. }
                | Op::JmpSEq { target, .. }
                | Op::JmpSNe { target, .. } => std::slice::from_ref(target),
                Op::Switch { table, .. } => &self.switch_tables[*table as usize].targets[..],
                _ => continue,
            };
            for &target in targets {
                let pc = target as usize;
                if funcs_by_pc.contains_key(&u32::from(target)) {
                    continue;
                }
                let name = containing_func(pc)
                    .map(crate::CcFunc::name)
                    .unwrap_or("bytecode");
                block_labels
                    .entry(pc)
                    .or_insert_with(|| format!("{name}.bb_{pc:04x}"));
            }
        }

        let native_names: HashMap<u16, String> = self
            .cc
            .functions
            .values()
            .filter_map(|f| match f {
                crate::CcFunc::Native { idx, name, .. } => Some((*idx, format!("jit_{name}"))),
                crate::CcFunc::Bc { .. } => None,
            })
            .collect();

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
                Op::IMod { dst, lhs, rhs } => format!("imod r{dst}, r{lhs}, r{rhs}"),
                Op::IModI { dst, lhs, imm } => format!("imod_imm r{dst}, r{lhs}, #{imm}"),
                Op::IAnd { dst, lhs, rhs } => format!("iand r{dst}, r{lhs}, r{rhs}"),
                Op::IAndI { dst, lhs, imm } => format!("iand_imm r{dst}, r{lhs}, #{imm}"),
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
                Op::DLe { dst, lhs, rhs } => format!("dle r{dst}, r{lhs}, r{rhs}"),
                Op::DGe { dst, lhs, rhs } => format!("dge r{dst}, r{lhs}, r{rhs}"),
                Op::DGt { dst, lhs, rhs } => format!("dgt r{dst}, r{lhs}, r{rhs}"),
                Op::BEq { dst, lhs, rhs } => format!("beq r{dst}, r{lhs}, r{rhs}"),
                Op::SEq { dst, lhs, rhs } => format!("seq r{dst}, r{lhs}, r{rhs}"),
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
                Op::Assert { cond, check } => format!("assert r{cond}, {check:?}"),
                Op::JmpF { cond, target } => format!(
                    "jmpf r{cond}, {target:04x} <{}>",
                    target_label(*target, cur_func)
                ),
                Op::JmpEqI { lhs, imm, target } => format!(
                    "jmpeq_imm r{lhs}, #{imm}, {target:04x} <{}>",
                    target_label(*target, cur_func)
                ),
                Op::JmpNeI { lhs, imm, target } => format!(
                    "jmpne_imm r{lhs}, #{imm}, {target:04x} <{}>",
                    target_label(*target, cur_func)
                ),
                Op::JmpSEq { lhs, rhs, target } => format!(
                    "jmpseq r{lhs}, r{rhs}, {target:04x} <{}>",
                    target_label(*target, cur_func)
                ),
                Op::JmpSNe { lhs, rhs, target } => format!(
                    "jmpsne r{lhs}, r{rhs}, {target:04x} <{}>",
                    target_label(*target, cur_func)
                ),
                Op::Lookup {
                    kind,
                    dst,
                    subject,
                    table,
                } => format!(
                    "lookup.{} r{dst}, r{subject}, {} slots",
                    match kind {
                        SwitchKind::Str => "str",
                        SwitchKind::Int => "int",
                    },
                    self.lookup_tables[*table as usize].values.len()
                ),
                Op::Switch {
                    kind,
                    subject,
                    table,
                } => format!(
                    "switch.{} r{subject}, {} slots",
                    match kind {
                        SwitchKind::Str => "str",
                        SwitchKind::Int => "int",
                    },
                    self.switch_tables[*table as usize].targets.len()
                ),
                Op::Call { func } => format!(
                    "call {func:04x} <{}>",
                    funcs_by_pc.get(func).unwrap().name()
                ),
                Op::Native { idx, .. } => format!(
                    "native {idx} <{}>",
                    native_names.get(idx).map_or("???", String::as_str),
                ),
                Op::Sys { idx, .. } => format!(
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
                Op::StoreSlot { slot, src } => format!("store_slot s{slot}, r{src}"),
                Op::LoadSlot { dst, slot } => format!("load_slot r{dst}, s{slot}"),
                Op::Alloc {
                    dst,
                    kind,
                    size,
                    align,
                } => format!("alloc r{dst}, {kind:?}, #{size}, #{align}"),
                Op::Store { base, offset, src } => {
                    format!("store r{base}, #{offset}, r{src}")
                }
                Op::Load { dst, base, offset } => {
                    format!("load r{dst}, r{base}, #{offset}")
                }
                Op::AddrOf { dst, base, offset } => {
                    format!("addrof r{dst}, r{base}, #{offset}")
                }
                Op::Ret => "ret".into(),
                Op::Halt => "halt".into(),
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
        let mut lines = Vec::with_capacity(self.bc.len() + funcs_by_pc.len());
        for (pc, instr) in self.bc.iter().enumerate() {
            if let Some(func) = funcs_by_pc.get(&(pc as u32)) {
                cur_func = func;
                lines.push(Line::Func {
                    pc,
                    name: func.name().to_string(),
                });
            }
            if let Some(label) = block_labels.get(&pc) {
                lines.push(Line::Block {
                    pc,
                    label: label.clone(),
                });
            }
            lines.push(Line::Op {
                pc,
                text: render(instr, cur_func),
            });
        }
        lines
    }

    pub fn disassemble_bytecode(&self) {
        let color = purple_garden_shared::ansi::enabled();

        let globals = self.globals();
        if !globals.is_empty() {
            println!("{}", paint(color, Class::Section, "globals:"));
            for (i, g) in globals.iter().enumerate() {
                println!(
                    "  {}:    {}",
                    paint(color, Class::Addr, &format!("{i:04}")),
                    paint(color, Class::Imm, g)
                );
            }
        }

        let lines = self.lines();
        let width = lines
            .iter()
            .filter_map(|line| match line {
                Line::Op { text, .. } => Some(text.len()),
                _ => None,
            })
            .max()
            .unwrap_or(0);

        let mut last_source_line = None;
        let mut func_def_line = None;
        let mut func_pc = None;
        for line in &lines {
            let (pc, instr) = match line {
                Line::Func { pc, name } => {
                    println!(
                        "\n{:08x} {}:",
                        pc,
                        paint(color, Class::Func, &format!("<{name}>"))
                    );
                    last_source_line = None;
                    func_def_line = self.source_line(*pc).map(|(n, _)| n);
                    func_pc = Some(*pc);
                    continue;
                }
                Line::Block { pc, label } => {
                    println!(
                        "{pc:08x} {}:",
                        paint(color, Class::Block, &format!("<{label}>"))
                    );
                    continue;
                }
                Line::Op { pc, text } => (*pc, text),
            };

            let is_entry = func_pc == Some(pc);
            let addr = paint(color, Class::Addr, &format!("{pc:04x}:"));
            let hex = self.bc[pc]
                .encode()
                .to_le_bytes()
                .map(|byte| format!("{byte:02x}"))
                .join(" ");
            let body = paint_highlights(color, instr, &highlight_instr(instr));
            // the return-value move inherits the function's definition span; the
            // prologue already shows that line, so don't re-attach it mid-body.
            if let Some((line_no, line)) = self.source_line(pc)
                && last_source_line != Some((line_no, line))
                && !(!is_entry && Some(line_no) == func_def_line)
            {
                let pad = " ".repeat(width.saturating_sub(instr.len()));
                let note = paint(color, Class::Comment, &format!("; {line_no}: {line}"));
                println!("  {addr}    {hex}    {body}{pad} {note}");
                last_source_line = Some((line_no, line));
            } else {
                println!("  {addr}    {hex}    {body}");
            }
        }
    }

    pub fn disassemble_native(&self) {
        let color = purple_garden_shared::ansi::enabled();
        let mut base = 0usize;

        for (name, _, code, _) in self.cc.native_code.as_deref().unwrap_or_default() {
            println!(
                "\n{base:08x} {}:",
                paint(color, Class::Func, &format!("<jit_{name}>"))
            );
            for (line, chunk) in code.chunks(16).enumerate() {
                let addr = base + line * 16;
                let hex = chunk
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!(
                    "  {}    {hex}",
                    paint(color, Class::Addr, &format!("{addr:04x}:"))
                );
            }
            base += code.len();
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
