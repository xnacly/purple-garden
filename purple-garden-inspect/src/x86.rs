//! Compiles every function with the x86-64 backend, on any host: the code is laid out like the
//! code arena would, but only disassembled, never run.

use std::{
    alloc::{Allocator, Global},
    collections::HashMap,
    ops::Range,
    time::{Duration, Instant},
};

use iced_x86::{
    Decoder, DecoderOptions, Formatter, Instruction, IntelFormatter, SymbolResolver, SymbolResult,
};
use purple_garden_ir::{Func, Id};
use purple_garden_jit::{Natives, x86};
use purple_garden_shared::xralloc2::{Location as Loc, Reg, Xralloc2};

use crate::model::{AsmLine, Location, Native};

/// Where the first function is laid out.
const BASE: u64 = 0x1000;
/// The code arena aligns every entry to this.
const ALIGN: usize = 16;

const GPRS: [&str; 16] = [
    "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13",
    "r14", "r15",
];

/// The x86-64 name of a register xralloc2 handed out.
#[must_use]
pub fn location(loc: Loc) -> Location {
    match loc {
        Loc::Reg(Reg::Gpr(num)) => Location::Reg(GPRS[num as usize].to_string()),
        Loc::Reg(Reg::Fpr(num)) => Location::Reg(format!("xmm{num}")),
        Loc::Stack(slot) => Location::Slot(slot),
    }
}

/// Compiles `ir` in order, callees before their callers so calls between natives resolve.
/// `globals` and `strings` are the bytecode compiler's constant slots and string addresses.
/// Returns the time code generation took next to the functions, disassembling them excluded.
pub fn compile<'ir, F: Allocator>(
    ir: &[Func<'ir, F>],
    globals: &HashMap<purple_garden_ir::Const<'ir>, u32>,
    strings: &[purple_garden_runtime::Value],
) -> (HashMap<Id, Result<Native, String>>, Duration) {
    let mut codegen = Duration::ZERO;
    let mut out = HashMap::with_capacity(ir.len());
    let mut natives: Natives<Global> = HashMap::new();
    let names: HashMap<Id, &str> = ir.iter().map(|f| (f.id, f.name)).collect();
    let mut ra = Xralloc2::new_in(Global);
    let mut buffers = x86::Scratch::new_in(Global);
    let mut liveness = Vec::new();
    let mut code = Vec::new();
    let mut at = BASE as usize;

    for func in ir.iter().filter(|f| !f.unused) {
        func.live_set_into(&mut liveness);
        code.clear();
        let start = Instant::now();
        let compiled = x86::compile_func(
            func,
            &mut code,
            &liveness,
            globals,
            strings,
            &natives,
            at,
            &mut ra,
            &mut buffers,
        );
        codegen += start.elapsed();
        let compiled = compiled.map(|()| {
            let mut symbols: HashMap<u64, String> = natives
                .iter()
                .map(|(id, &addr)| (addr as u64, format!("f{} <{}>", id.0, names[id])))
                .collect();
            symbols.insert(at as u64, format!("f{} <{}>", func.id.0, func.name));
            let mut labels: HashMap<usize, String> = HashMap::new();
            for (pos, block) in block_rows(func) {
                if let Some(&(_, offset)) = buffers.rows().iter().find(|&&(p, _)| p == pos) {
                    symbols
                        .entry(at as u64 + offset as u64)
                        .or_insert(format!("b{}", block.0));
                    labels.entry(offset).or_insert(format!("b{}", block.0));
                }
            }
            labels.insert(0, format!("f{} <{}>", func.id.0, func.name));
            Native {
                address: at as u64,
                size: code.len(),
                lines: disassemble(
                    &code,
                    at as u64,
                    buffers.tables(),
                    buffers.rows(),
                    &labels,
                    Symbols {
                        labels: symbols,
                        func: (at as u64..(at + code.len()) as u64, func.name.to_string()),
                        offset: String::new(),
                    },
                ),
                locations: (0..liveness.len())
                    .map(|id| ra.location(id).map(location))
                    .collect(),
            }
        });
        if compiled.is_ok() {
            natives.insert(func.id, at);
            at = (at + code.len()).next_multiple_of(ALIGN);
        }
        out.insert(func.id, compiled.map_err(|reason| reason.to_string()));
    }
    (out, codegen)
}

/// `(pos, block)` of every live block's label row, see [`Func::live_set_into`].
fn block_rows<F: Allocator>(func: &Func<'_, F>) -> Vec<(u32, Id)> {
    let mut pos = 0;
    let mut out = Vec::new();
    for block in func.blocks.iter().filter(|b| !b.tombstone) {
        out.push((pos, block.id));
        pos += block.instructions.len() as u32 + 2;
    }
    out
}

/// Resolves jump and call targets to block and function labels, other addresses in the function
/// to their offset from its start.
struct Symbols {
    labels: HashMap<u64, String>,
    /// The function being disassembled and its name.
    func: (Range<u64>, String),
    /// The text of the last resolved offset, the result borrows it.
    offset: String,
}

impl SymbolResolver for Symbols {
    fn symbol(
        &mut self,
        _: &Instruction,
        _: u32,
        _: Option<u32>,
        address: u64,
        _: u32,
    ) -> Option<SymbolResult<'_>> {
        if let Some(text) = self.labels.get(&address) {
            return Some(SymbolResult::with_str(address, text));
        }
        let (range, name) = &self.func;
        if !range.contains(&address) {
            return None;
        }
        self.offset = format!("{name}+0x{:x}", address - range.start);
        Some(SymbolResult::with_str(address, &self.offset))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The row whose code the byte at `offset` belongs to: the last one starting at or before it.
fn row_at(rows: &[(u32, usize)], offset: usize) -> Option<u32> {
    let idx = rows.partition_point(|&(_, start)| start <= offset);
    idx.checked_sub(1).map(|i| rows[i].0)
}

fn disassemble(
    code: &[u8],
    address: u64,
    tables: &[Range<usize>],
    rows: &[(u32, usize)],
    labels: &HashMap<usize, String>,
    symbols: Symbols,
) -> Vec<AsmLine> {
    let mut formatter = IntelFormatter::with_options(Some(Box::new(symbols)), None);
    formatter.options_mut().set_first_operand_char_index(8);
    formatter.options_mut().set_hex_prefix("0x");
    formatter.options_mut().set_hex_suffix("");
    formatter.options_mut().set_uppercase_hex(false);
    formatter
        .options_mut()
        .set_space_after_operand_separator(true);
    formatter.options_mut().set_show_branch_size(false);
    formatter.options_mut().set_leading_zeros(false);
    formatter.options_mut().set_branch_leading_zeros(false);

    let mut tables = tables.to_vec();
    tables.sort_by_key(|t| t.start);
    let mut lines = Vec::new();
    let mut at = 0;
    let mut code_segment = |range: Range<usize>, lines: &mut Vec<AsmLine>| {
        let mut decoder = Decoder::with_ip(
            64,
            &code[range.clone()],
            address + range.start as u64,
            DecoderOptions::NONE,
        );
        let mut instruction = Instruction::default();
        while decoder.can_decode() {
            decoder.decode_out(&mut instruction);
            let offset = (instruction.ip() - address) as usize;
            let mut text = String::new();
            formatter.format(&instruction, &mut text);
            lines.push(AsmLine {
                offset,
                bytes: hex(&code[offset..offset + instruction.len()]),
                text,
                label: labels.get(&offset).cloned(),
                data: false,
                row: row_at(rows, offset),
            });
        }
    };
    for table in tables.iter().filter(|t| t.start < t.end) {
        if at < table.start {
            code_segment(at..table.start, &mut lines);
        }
        for start in table.clone().step_by(8) {
            let end = (start + 8).min(table.end);
            lines.push(AsmLine {
                offset: start,
                bytes: hex(&code[start..end]),
                text: format!(".byte {}", hex(&code[start..end])),
                label: labels.get(&start).cloned(),
                data: true,
                row: row_at(rows, start),
            });
        }
        at = table.end;
    }
    if at < code.len() {
        code_segment(at..code.len(), &mut lines);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_at_picks_the_last_row_starting_before() {
        let rows = [(0, 4), (1, 4), (2, 9), (3, 12)];
        assert_eq!(row_at(&rows, 0), None, "the frame belongs to no row");
        assert_eq!(row_at(&rows, 3), None);
        // rows 0 and 1 start at 4, row 0 emitted nothing
        assert_eq!(row_at(&rows, 4), Some(1));
        assert_eq!(row_at(&rows, 8), Some(1));
        assert_eq!(row_at(&rows, 9), Some(2));
        assert_eq!(row_at(&rows, 100), Some(3));
        assert_eq!(row_at(&[], 3), None);
    }

    #[test]
    fn disassembles_around_inlined_tables() {
        // mov eax, 1; <table of 8 bytes>; ret
        let mut code = vec![0xb8, 1, 0, 0, 0];
        code.extend_from_slice(&[0xaa; 8]);
        code.push(0xc3);
        let lines = disassemble(
            &code,
            0x1000,
            std::slice::from_ref(&(5..13)),
            &[(0, 0), (1, 13)],
            &HashMap::from([(0, "f0 <f>".to_string())]),
            Symbols {
                labels: HashMap::new(),
                func: (0x1000..0x1000 + code.len() as u64, "f".to_string()),
                offset: String::new(),
            },
        );
        let texts: Vec<_> = lines
            .iter()
            .map(|l| (l.offset, l.data, l.text.as_str()))
            .collect();
        assert_eq!(
            texts,
            [
                (0, false, "mov     eax, 1"),
                (5, true, ".byte aa aa aa aa aa aa aa aa"),
                (13, false, "ret"),
            ]
        );
        assert_eq!(lines[0].label.as_deref(), Some("f0 <f>"));
        assert_eq!(lines[2].row, Some(1));
    }

    #[test]
    fn jumps_resolve_to_labels_then_offsets() {
        // jmp +0 (to the ret); ret; call far outside the function
        let code = [0xeb, 0x00, 0xc3, 0xe8, 0x00, 0x10, 0x00, 0x00];
        let symbols = |labels: HashMap<u64, String>| Symbols {
            labels,
            func: (0x1000..0x1000 + code.len() as u64, "f".to_string()),
            offset: String::new(),
        };
        let lines = disassemble(
            &code,
            0x1000,
            &[],
            &[],
            &HashMap::new(),
            symbols(HashMap::new()),
        );
        assert_eq!(lines[0].text, "jmp     f+0x2");
        assert_eq!(
            lines[2].text, "call    0x2008",
            "outside the function stays an address"
        );
        let lines = disassemble(
            &code,
            0x1000,
            &[],
            &[],
            &HashMap::new(),
            symbols(HashMap::from([(0x1002, "b1".to_string())])),
        );
        assert_eq!(lines[0].text, "jmp     b1");
    }
}
