use std::io::{self, Write};

// INFO: this sucks to look at but encoding elf64 requires the layout, it is what it is.

const EHDR: usize = 64;
const SHDR: usize = 64;
const SYM: usize = 24;

#[cfg(target_arch = "x86_64")]
const EM: u16 = 62;
#[cfg(target_arch = "aarch64")]
const EM: u16 = 183;

const TEXT: u16 = 1;
const SHSTRTAB: u16 = 4;

pub fn write(native_code: &[(&str, Vec<u8>)], mut out: impl Write) -> io::Result<()> {
    let mut buf = vec![0; EHDR];

    pad(&mut buf, 16);
    let text_off = buf.len();
    let mut funcs = Vec::with_capacity(native_code.len());
    for (name, code) in native_code {
        let off = buf.len() - text_off;
        buf.extend_from_slice(code);
        funcs.push((*name, off, code.len()));
    }

    pad(&mut buf, 8);
    let symtab_off = buf.len();
    buf.resize(buf.len() + SYM, 0);

    let mut strtab = vec![0];
    for (name, off, size) in &funcs {
        let name_off = strtab.len() as u32;
        strtab.extend_from_slice(b"jit_");
        strtab.extend_from_slice(name.as_bytes());
        strtab.push(0);
        sym(&mut buf, name_off, TEXT, *off as u64, *size as u64);
    }
    let text_size: usize = funcs.iter().map(|f| f.2).sum();
    let symtab_size = buf.len() - symtab_off;

    let strtab_off = buf.len();
    buf.extend_from_slice(&strtab);

    let shstrtab = b"\0.text\0.symtab\0.strtab\0.shstrtab\0";
    let text_name = 1;
    let symtab_name = 7;
    let strtab_name = 15;
    let shstrtab_name = 23;
    let shstrtab_off = buf.len();
    buf.extend_from_slice(shstrtab);

    pad(&mut buf, 8);
    let shoff = buf.len();
    buf.resize(buf.len() + 5 * SHDR, 0);

    shdr(
        &mut buf[shoff + SHDR..shoff + 2 * SHDR],
        text_name,
        1,
        0x6,
        text_off,
        text_size,
        0,
        0,
        16,
        0,
    );
    shdr(
        &mut buf[shoff + 2 * SHDR..shoff + 3 * SHDR],
        symtab_name,
        2,
        0,
        symtab_off,
        symtab_size,
        3,
        1,
        8,
        SYM as u64,
    );
    shdr(
        &mut buf[shoff + 3 * SHDR..shoff + 4 * SHDR],
        strtab_name,
        3,
        0,
        strtab_off,
        strtab.len(),
        0,
        0,
        1,
        0,
    );
    shdr(
        &mut buf[shoff + 4 * SHDR..shoff + 5 * SHDR],
        shstrtab_name,
        3,
        0,
        shstrtab_off,
        shstrtab.len(),
        0,
        0,
        1,
        0,
    );

    ehdr(&mut buf[..EHDR], shoff);
    out.write_all(&buf)
}

macro_rules! put {
    ($out:expr, @at $off:expr, $v:expr, $ty:ty) => {{
        $out[$off..$off + core::mem::size_of::<$ty>()].copy_from_slice(&($v as $ty).to_le_bytes());
    }};
    ($out:expr, @push $v:expr, $ty:ty) => {{
        $out.extend_from_slice(&($v as $ty).to_le_bytes());
    }};
}

fn ehdr(out: &mut [u8], shoff: usize) {
    out[..16].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    put!(out, @at 16, 1, u16);
    put!(out, @at 18, EM, u16);
    put!(out, @at 20, 1, u32);
    put!(out, @at 40, shoff as u64, u64);
    put!(out, @at 52, EHDR as u16, u16);
    put!(out, @at 58, SHDR as u16, u16);
    put!(out, @at 60, 5, u16);
    put!(out, @at 62, SHSTRTAB, u16);
}

fn shdr(
    out: &mut [u8],
    name: u32,
    ty: u32,
    flags: u64,
    off: usize,
    size: usize,
    link: u32,
    info: u32,
    align: u64,
    entsize: u64,
) {
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

fn sym(out: &mut Vec<u8>, name: u32, shndx: u16, value: u64, size: u64) {
    put!(out, @push name, u32);
    out.push(0x12);
    out.push(0);
    put!(out, @push shndx, u16);
    put!(out, @push value, u64);
    put!(out, @push size, u64);
}

fn pad(out: &mut Vec<u8>, n: usize) {
    out.resize(out.len().next_multiple_of(n), 0);
}
