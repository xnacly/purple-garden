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
        let mut buf = [0u8; 8];
        buf[..tail.len()].copy_from_slice(tail);
        h = (h.rotate_left(5) ^ u64::from_le_bytes(buf)).wrapping_mul(K);
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
            if unsafe { header_bits(v.as_ptr()) } == word && v.as_str() == s {
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
