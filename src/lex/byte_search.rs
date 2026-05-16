#[inline]
pub fn find_byte(needle: u8, haystack: &[u8]) -> Option<usize> {
    imp::find_byte(needle, haystack)
}

#[cfg(target_arch = "x86_64")]
mod imp {
    use core::arch::asm;

    #[inline]
    pub fn find_byte(needle: u8, haystack: &[u8]) -> Option<usize> {
        // byte offset into haystack
        let mut i = 0;
        // SIMD while 16-byte block avail
        while haystack.len().saturating_sub(i) >= 16 {
            // bitmask for cur block: bit N means byte N found
            let mask = unsafe { eq_mask_16(haystack.as_ptr().add(i), needle) };
            if mask != 0 {
                // lowest set bit is the first match
                return Some(i + mask.trailing_zeros() as usize);
            }
            // No match, lets check next
            i += 16;
        }

        // process < 16 bytes normally
        scalar_tail(needle, haystack, i)
    }

    #[inline(always)]
    unsafe fn eq_mask_16(ptr: *const u8, needle: u8) -> u32 {
        // Compare 16 bytes at ptr against needle. Bit N in the returned
        // mask is set when ptr[N] == needle.
        let mask: u32;
        unsafe {
            asm!(
                // Move needle into the low 32 bits of xmm1.
                "movd xmm1, {needle:e}",
                // Duplicate low byte into low two byte lanes.
                "punpcklbw xmm1, xmm1",
                // Duplicate bytes into the low four byte lanes.
                "punpcklwd xmm1, xmm1",
                // Push low 32 bits across all 16 byte lanes.
                "pshufd xmm1, xmm1, 0",
                // Load 16 unaligned haystack bytes.
                "movdqu xmm0, [{ptr}]",
                // Per byte equality: matching: 0xff; others: 0x00.
                "pcmpeqb xmm0, xmm1",
                // Pack lane high bit into mask; bit N maps to byte N.
                "pmovmskb {mask:e}, xmm0",
                needle = in(reg) needle as u32,
                ptr = in(reg) ptr,
                mask = lateout(reg) mask,
                out("xmm0") _,
                out("xmm1") _,
                options(nostack, readonly, preserves_flags),
            );
        }
        mask
    }

    #[inline]
    fn scalar_tail(needle: u8, haystack: &[u8], start: usize) -> Option<usize> {
        let mut i = start;
        while i < haystack.len() {
            if haystack[i] == needle {
                return Some(i);
            }
            i += 1;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::find_byte;

    #[test]
    fn finds_matches_in_head_middle_and_tail() {
        assert_eq!(find_byte(b'x', b"xabcdefghijklmnop"), Some(0));
        assert_eq!(find_byte(b'x', b"abcdefghijklmnopx"), Some(16));
        assert_eq!(find_byte(b'x', b"abcdefghijklmnopqrstuvwxyz"), Some(23));
    }

    #[test]
    fn reports_absent_and_empty() {
        assert_eq!(find_byte(b'x', b""), None);
        assert_eq!(find_byte(b'x', b"abcdefghijklmnopqrstuvw"), None);
    }

    #[test]
    fn returns_first_match() {
        assert_eq!(find_byte(b'x', b"abcxdefxghi"), Some(3));
    }
}

// TODO: i dont fucking know enough about neon to do this
#[cfg(target_arch = "aarch64")]
mod imp {
    #[inline]
    pub fn find_byte(_needle: u8, _haystack: &[u8]) -> Option<usize> {
        todo!("aarch64 SIMD byte search")
    }
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
mod imp {
    #[inline]
    pub fn find_byte(needle: u8, haystack: &[u8]) -> Option<usize> {
        let mut i = 0;
        while i < haystack.len() {
            if haystack[i] == needle {
                return Some(i);
            }
            i += 1;
        }
        None
    }
}
