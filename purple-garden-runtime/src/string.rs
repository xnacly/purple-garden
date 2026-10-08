//! String payload layout shared by the GC heap and the compilers const pool
use crate::Value;

#[repr(C)]
pub struct StrHeader {
    pub len: u32,
    pub hash: u32,
}

pub const HEADER_SIZE: usize = std::mem::size_of::<StrHeader>();
pub const ALIGN: usize = std::mem::align_of::<StrHeader>();

#[inline]
pub const fn payload_size(len: usize) -> usize {
    HEADER_SIZE + len
}

/// [`payload_size`] rounded up so the next entry in a packed buffer stays
/// [`ALIGN`]-aligned.
#[inline]
pub const fn padded_size(len: usize) -> usize {
    payload_size(len).next_multiple_of(ALIGN)
}

/// FxHash over 8-byte chunks, length seed & folded to 32 bits.
#[inline]
pub fn hash(bytes: &[u8]) -> u32 {
    const K: u64 = 0x517c_c1b7_2722_0a95;
    let mut h = bytes.len() as u64;
    let (chunks, tail) = bytes.as_chunks::<8>();
    for c in chunks {
        h = (h.rotate_left(5) ^ u64::from_le_bytes(*c)).wrapping_mul(K);
    }
    if !tail.is_empty() {
        // Assembled in a register: copying into a stack buffer and loading it
        // as one u64 stalls on store forwarding.
        let tail = tail
            .iter()
            .enumerate()
            .fold(0u64, |acc, (i, &b)| acc | u64::from(b) << (8 * i));
        h = (h.rotate_left(5) ^ tail).wrapping_mul(K);
    }
    (h ^ (h >> 32)) as u32
}

/// # Safety
///
/// `ptr` must point at a string payload.
#[inline(always)]
pub unsafe fn header_bits(ptr: *const u8) -> u64 {
    const _: () = assert!(HEADER_SIZE == std::mem::size_of::<u64>());
    unsafe { (ptr as *const u64).read_unaligned() }
}

/// The `[len | hash]` word [`header_bits`] reads for such a header.
#[inline]
pub fn header_word(len: u32, hash: u32) -> u64 {
    const _: () = assert!(cfg!(target_endian = "little"));
    u64::from(len) | (u64::from(hash) << 32)
}

/// Writes header and bytes of `s` to `dst` and returns the string `Value`.
/// `hash` is [`hash`] of `s`, computed by the caller for the intern lookup.
///
/// # Safety
///
/// `dst` must be [`ALIGN`]-aligned and valid for `payload_size(s.len())` bytes
/// of writes.
#[inline]
pub unsafe fn write_payload(dst: *mut u8, s: &str, hash: u32) -> Value {
    let bytes = s.as_bytes();
    let len: u32 = bytes.len().try_into().expect("string length exceeds 4 GiB");
    unsafe {
        (dst as *mut StrHeader).write(StrHeader { len, hash });
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst.add(HEADER_SIZE), bytes.len());
    }
    Value::from_ptr(dst)
}

/// Equality of two equally long byte strings without a call into libc's
/// memcmp, which branches on the length to pick a strategy and mispredicts on
/// mixed lengths. Short strings take two overlapping loads.
#[inline]
fn same_bytes(a: &[u8], b: &[u8]) -> bool {
    debug_assert_eq!(a.len(), b.len());
    let len = a.len();
    let u64_at = |s: &[u8], at: usize| u64::from_le_bytes(s[at..at + 8].try_into().unwrap());
    let u32_at = |s: &[u8], at: usize| u32::from_le_bytes(s[at..at + 4].try_into().unwrap());
    match len {
        0 => true,
        1..=3 => (a[0] == b[0]) & (a[len / 2] == b[len / 2]) & (a[len - 1] == b[len - 1]),
        4..=7 => (u32_at(a, 0) ^ u32_at(b, 0)) | (u32_at(a, len - 4) ^ u32_at(b, len - 4)) == 0,
        8..=16 => (u64_at(a, 0) ^ u64_at(b, 0)) | (u64_at(a, len - 8) ^ u64_at(b, len - 8)) == 0,
        _ => a == b,
    }
}

/// Every string the VM knows, so equal contents share one payload and string
/// equality is a pointer compare. Open addressing on the header hash with
/// linear probing, kept at most half full.
///
/// TODO: the GC doesn't free yet; once it sweeps, dead strings have to leave
/// this table.
#[derive(Debug, Default)]
pub struct StrTable {
    /// [`Value::UNDEF`] marks an empty slot, payload pointers are never null.
    slots: Vec<Value>,
    len: usize,
}

impl StrTable {
    pub fn get(&self, s: &str, hash: u32) -> Option<Value> {
        if self.slots.is_empty() {
            return None;
        }
        let word = header_word(s.len() as u32, hash);
        let mask = self.slots.len() - 1;
        let mut i = hash as usize & mask;
        loop {
            let v = self.slots[i];
            if v == Value::UNDEF {
                return None;
            }
            if unsafe { header_bits(v.as_ptr()) } == word
                && same_bytes(v.as_str().as_bytes(), s.as_bytes())
            {
                return Some(v);
            }
            i = (i + 1) & mask;
        }
    }

    /// `v` must not be in the table yet.
    pub fn insert(&mut self, v: Value) {
        if (self.len + 1) * 2 > self.slots.len() {
            let cap = (self.slots.len() * 2).max(64);
            let old = std::mem::replace(&mut self.slots, vec![Value::UNDEF; cap]);
            for v in old.into_iter().filter(|&v| v != Value::UNDEF) {
                self.place(v);
            }
        }
        self.place(v);
        self.len += 1;
    }

    fn place(&mut self, v: Value) {
        let mask = self.slots.len() - 1;
        let mut i = v.str_hash() as usize & mask;
        while self.slots[i] != Value::UNDEF {
            i = (i + 1) & mask;
        }
        self.slots[i] = v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_bytes_sees_every_byte() {
        for len in 0..40 {
            let a: Vec<u8> = (0..len as u8).collect();
            assert!(same_bytes(&a, &a.clone()), "len {len}");
            for at in 0..len {
                let mut b = a.clone();
                b[at] ^= 0x80;
                assert!(!same_bytes(&a, &b), "len {len}, differs at {at}");
            }
        }
    }

    /// The tail chunk is zero padded, so only the length seed separates these.
    #[test]
    fn hash_separates_zero_padded_tails() {
        assert_ne!(hash(b""), hash(b"\0"));
        assert_ne!(hash(b"garden"), hash(b"garden\0"));
    }

    #[test]
    fn payload_roundtrip() {
        let mut buf = vec![0u8; padded_size(5)];
        let v = unsafe { write_payload(buf.as_mut_ptr(), "hello", hash(b"hello")) };
        assert_eq!(v.as_str(), "hello");
        assert_eq!(v.str_hash(), hash(b"hello"));
    }
}
