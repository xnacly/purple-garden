use std::{
    alloc::Allocator,
    collections::HashMap,
    io::{self, Write},
};

use purple_garden_bc::{Cc, CcFunc, dis::Disassembler};
use purple_garden_runtime::BuiltinFn;

// INFO: this sucks to look at but encoding elf64 requires the layout, it is what it is.

macro_rules! put {
    ($out:expr, @at $off:expr, $v:expr, $ty:ty) => {{
        $out[$off..$off + core::mem::size_of::<$ty>()].copy_from_slice(&($v as $ty).to_le_bytes());
    }};
    ($out:expr, @push $v:expr, $ty:ty) => {{
        $out.extend_from_slice(&($v as $ty).to_le_bytes());
    }};
}

const EHDR: usize = 64;
const SHDR: usize = 64;
const SYM: usize = 24;

#[cfg(target_arch = "x86_64")]
const EM: u16 = 62;
#[cfg(target_arch = "aarch64")]
const EM: u16 = 183;

const RELA: usize = 24;

const TEXT: u16 = 1;
const SYMTAB: u32 = 2;
const SHSTRTAB: u16 = 5;
const SECTIONS: usize = 6;

const R_X86_64_64: u64 = 1;

/// Names of the runtime addresses the jit embeds as `movabs` immediates: the helpers, builtins
/// and the entries of native functions.
pub(crate) fn addresses<S: Allocator + Clone, A: Allocator + Clone>(
    cc: &Cc<'_, S, A>,
    syscalls: &[BuiltinFn],
) -> HashMap<u64, String> {
    let mut names: HashMap<u64, String> = Disassembler::<S, A>::build_fn_map()
        .into_iter()
        .map(|(f, name)| (f as usize as u64, name))
        .collect();
    for (f, name) in [
        (
            purple_garden_runtime::jit_sys as *const () as usize,
            "jit_sys",
        ),
        (
            purple_garden_runtime::jit_alloc as *const () as usize,
            "jit_alloc",
        ),
        (
            purple_garden_runtime::jit_trap_div_zero as *const () as usize,
            "jit_trap_div_zero",
        ),
        (
            purple_garden_runtime::jit_trap_stack_overflow as *const () as usize,
            "jit_trap_stack_overflow",
        ),
    ] {
        names.insert(f as u64, name.to_string());
    }
    for f in cc.functions.values() {
        if let CcFunc::Native { name, idx, .. } = f {
            names.insert(
                syscalls[*idx as usize] as usize as u64,
                format!("jit_{name}"),
            );
        }
    }
    names
}

/// Writes `native_code` as a relocatable object, every embedded address `addresses` names as a
/// relocation against that name, so `objdump -dr` shows it below its `movabs`.
pub(crate) fn write(
    native_code: &[(&str, Vec<u8>)],
    addresses: &HashMap<u64, String>,
    mut out: impl Write,
) -> io::Result<()> {
    let mut buf = vec![0; EHDR];

    pad(&mut buf, 16);
    let text_off = buf.len();
    let mut funcs = Vec::with_capacity(native_code.len());
    for (name, code) in native_code {
        let off = buf.len() - text_off;
        buf.extend_from_slice(code);
        funcs.push((format!("jit_{name}"), off, code.len()));
    }
    let text_size: usize = funcs.iter().map(|f| f.2).sum();

    // symbol 0 is the null symbol, the functions follow in order
    let mut symbols: HashMap<&str, u32> = funcs
        .iter()
        .enumerate()
        .map(|(i, f)| (f.0.as_str(), i as u32 + 1))
        .collect();
    let mut undefined = Vec::new();
    let mut relocs = Vec::new();
    let mut i = text_off;
    while i + 10 <= text_off + text_size {
        let Some(name) = movabs_immediate(&buf[i..i + 10]).and_then(|imm| addresses.get(&imm))
        else {
            i += 1;
            continue;
        };
        let next = (funcs.len() + undefined.len()) as u32 + 1;
        let sym = *symbols.entry(name.as_str()).or_insert_with(|| {
            undefined.push(name.as_str());
            next
        });
        // the address differs every run, the relocation carries it instead
        buf[i + 2..i + 10].fill(0);
        relocs.push(((i + 2 - text_off) as u64, sym));
        i += 10;
    }

    pad(&mut buf, 8);
    let symtab_off = buf.len();
    buf.resize(buf.len() + SYM, 0);

    let mut strtab = vec![0];
    let mut push_name = |name: &str| {
        let name_off = strtab.len() as u32;
        strtab.extend_from_slice(name.as_bytes());
        strtab.push(0);
        name_off
    };
    for (name, off, size) in &funcs {
        let name_off = push_name(name);
        sym(&mut buf, name_off, 0x12, TEXT, *off as u64, *size as u64);
    }
    for name in &undefined {
        let name_off = push_name(name);
        sym(&mut buf, name_off, 0x10, 0, 0, 0);
    }
    let symtab_size = buf.len() - symtab_off;

    let strtab_off = buf.len();
    buf.extend_from_slice(&strtab);

    pad(&mut buf, 8);
    let rela_off = buf.len();
    for (off, sym) in &relocs {
        put!(buf, @push *off, u64);
        put!(buf, @push (u64::from(*sym) << 32) | R_X86_64_64, u64);
        put!(buf, @push 0, i64);
    }
    let rela_size = buf.len() - rela_off;

    let shstrtab = b"\0.text\0.symtab\0.strtab\0.shstrtab\0.rela.text\0";
    let text_name = 1;
    let symtab_name = 7;
    let strtab_name = 15;
    let shstrtab_name = 23;
    let rela_name = 33;
    let shstrtab_off = buf.len();
    buf.extend_from_slice(shstrtab);

    pad(&mut buf, 8);
    let shoff = buf.len();
    buf.resize(buf.len() + SECTIONS * SHDR, 0);

    shdr(
        &mut buf[shoff + SHDR..shoff + 2 * SHDR],
        SectionHeader {
            name: text_name,
            ty: 1,
            flags: 0x6,
            off: text_off,
            size: text_size,
            link: 0,
            info: 0,
            align: 16,
            entsize: 0,
        },
    );
    shdr(
        &mut buf[shoff + 2 * SHDR..shoff + 3 * SHDR],
        SectionHeader {
            name: symtab_name,
            ty: 2,
            flags: 0,
            off: symtab_off,
            size: symtab_size,
            link: 3,
            info: 1,
            align: 8,
            entsize: SYM as u64,
        },
    );
    shdr(
        &mut buf[shoff + 3 * SHDR..shoff + 4 * SHDR],
        SectionHeader {
            name: strtab_name,
            ty: 3,
            flags: 0,
            off: strtab_off,
            size: strtab.len(),
            link: 0,
            info: 0,
            align: 1,
            entsize: 0,
        },
    );
    shdr(
        &mut buf[shoff + 4 * SHDR..shoff + 5 * SHDR],
        SectionHeader {
            name: rela_name,
            ty: 4,
            // SHF_INFO_LINK: info is the section the relocations apply to
            flags: 0x40,
            off: rela_off,
            size: rela_size,
            link: SYMTAB,
            info: u32::from(TEXT),
            align: 8,
            entsize: RELA as u64,
        },
    );
    shdr(
        &mut buf[shoff + 5 * SHDR..shoff + 6 * SHDR],
        SectionHeader {
            name: shstrtab_name,
            ty: 3,
            flags: 0,
            off: shstrtab_off,
            size: shstrtab.len(),
            link: 0,
            info: 0,
            align: 1,
            entsize: 0,
        },
    );

    ehdr(&mut buf[..EHDR], shoff);
    out.write_all(&buf)
}

fn ehdr(out: &mut [u8], shoff: usize) {
    out[..16].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    put!(out, @at 16, 1, u16);
    put!(out, @at 18, EM, u16);
    put!(out, @at 20, 1, u32);
    put!(out, @at 40, shoff as u64, u64);
    put!(out, @at 52, EHDR as u16, u16);
    put!(out, @at 58, SHDR as u16, u16);
    put!(out, @at 60, SECTIONS, u16);
    put!(out, @at 62, SHSTRTAB, u16);
}

struct SectionHeader {
    name: u32,
    ty: u32,
    flags: u64,
    off: usize,
    size: usize,
    link: u32,
    info: u32,
    align: u64,
    entsize: u64,
}

fn shdr(out: &mut [u8], header: SectionHeader) {
    let SectionHeader {
        name,
        ty,
        flags,
        off,
        size,
        link,
        info,
        align,
        entsize,
    } = header;
    put!(out, @at 0, name, u32);
    put!(out, @at 4, ty, u32);
    put!(out, @at 8, flags, u64);
    put!(out, @at 16, 0, u64);
    put!(out, @at 24, off as u64, u64);
    put!(out, @at 32, size as u64, u64);
    put!(out, @at 40, link, u32);
    put!(out, @at 44, info, u32);
    put!(out, @at 48, align, u64);
    put!(out, @at 56, entsize, u64);
}

/// `info` is binding << 4 | type: 0x12 a global function, 0x10 a global undefined name.
fn sym(out: &mut Vec<u8>, name: u32, info: u8, shndx: u16, value: u64, size: u64) {
    put!(out, @push name, u32);
    out.push(info);
    out.push(0);
    put!(out, @push shndx, u16);
    put!(out, @push value, u64);
    put!(out, @push size, u64);
}

/// The immediate if `insn` starts with `movabs r64, imm64` (`REX.W[.B] B8+r`). Matched on bytes,
/// not decoded, so it's only trusted for immediates that are known addresses.
fn movabs_immediate(insn: &[u8]) -> Option<u64> {
    (cfg!(target_arch = "x86_64") && matches!(insn[0], 0x48 | 0x49) && insn[1] & 0xf8 == 0xb8)
        .then(|| u64::from_le_bytes(insn[2..10].try_into().unwrap()))
}

fn pad(out: &mut Vec<u8>, n: usize) {
    out.resize(out.len().next_multiple_of(n), 0);
}
