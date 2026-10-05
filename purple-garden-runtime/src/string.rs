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

/// Writes header and bytes of `s` to `dst` and returns the string `Value`.
///
/// # Safety
///
/// `dst` must be [`ALIGN`]-aligned and valid for `payload_size(s.len())` bytes
/// of writes.
#[inline]
pub unsafe fn write_payload(dst: *mut u8, s: &str) -> Value {
    let bytes = s.as_bytes();
    let len: u32 = bytes.len().try_into().expect("string length exceeds 4 GiB");
    unsafe {
        (dst as *mut StrHeader).write(StrHeader {
            len,
            hash: hash(bytes),
        });
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst.add(HEADER_SIZE), bytes.len());
    }
    Value::from_ptr(dst)
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
        let v = unsafe { write_payload(buf.as_mut_ptr(), "hello") };
        assert_eq!(v.as_str(), "hello");
        assert_eq!(v.str_hash(), hash(b"hello"));
    }
}
