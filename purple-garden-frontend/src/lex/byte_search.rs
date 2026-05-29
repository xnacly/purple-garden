//! Byte-level scans for the lexer.
//!
//! The hot paths (single-byte search, ident/num class skip) use
//! `std::simd` so LLVM can hoist constants, schedule across the 16-byte
//! loop, and lower to SSE2 / NEON / WASM-SIMD per target.

#[inline]
pub fn find_byte(needle: u8, haystack: &[u8]) -> Option<usize> {
    imp::find_byte(needle, haystack)
}

/// Returns the offset of the first byte in `haystack` that is NOT an
/// identifier-continuation byte ({alpha, digit, '_'}). Returns
/// `haystack.len()` if every byte is one.
#[inline]
pub fn skip_ident_cont(haystack: &[u8]) -> usize {
    imp::skip_ident_cont(haystack)
}

/// Returns the offset of the first byte in `haystack` that is NOT a
/// number-continuation byte ({digit, '.'}). Returns `haystack.len()` if
/// every byte is one.
#[inline]
pub fn skip_num_cont(haystack: &[u8]) -> usize {
    imp::skip_num_cont(haystack)
}

mod imp {
    use std::simd::{
        cmp::{SimdPartialEq, SimdPartialOrd},
        u8x16,
    };

    pub fn find_byte(needle: u8, haystack: &[u8]) -> Option<usize> {
        let len = haystack.len();
        let mut i = 0;
        if len >= 16 {
            let n = u8x16::splat(needle);
            let limit = len - 15;
            while i < limit {
                let b = u8x16::from_slice(&haystack[i..i + 16]);
                let mask = b.simd_eq(n).to_bitmask() as u16;
                if mask != 0 {
                    return Some(i + mask.trailing_zeros() as usize);
                }
                i += 16;
            }
        }
        scalar_find(needle, haystack, i)
    }

    pub fn skip_ident_cont(haystack: &[u8]) -> usize {
        let len = haystack.len();
        let mut i = 0;
        if len >= 16 {
            // SimdPartialOrd on u8x16 is unsigned; ASCII range checks are
            // direct without any signed-byte trick. Non-ASCII bytes
            // (≥ 0x80) fall outside every ASCII range and correctly
            // classify as "not ident cont".
            let zero = u8x16::splat(b'0');
            let nine = u8x16::splat(b'9');
            let upper_a = u8x16::splat(b'A');
            let upper_z = u8x16::splat(b'Z');
            let lower_a = u8x16::splat(b'a');
            let lower_z = u8x16::splat(b'z');
            let under = u8x16::splat(b'_');
            let limit = len - 15;
            while i < limit {
                let b = u8x16::from_slice(&haystack[i..i + 16]);
                let digit = b.simd_ge(zero) & b.simd_le(nine);
                let upper = b.simd_ge(upper_a) & b.simd_le(upper_z);
                let lower = b.simd_ge(lower_a) & b.simd_le(lower_z);
                let u = b.simd_eq(under);
                let class = digit | upper | lower | u;
                let mask = class.to_bitmask() as u16;
                if mask != 0xFFFF {
                    return i + (!mask).trailing_zeros() as usize;
                }
                i += 16;
            }
        }
        scalar_skip_ident_cont(haystack, i)
    }

    pub fn skip_num_cont(haystack: &[u8]) -> usize {
        let len = haystack.len();
        let mut i = 0;
        if len >= 16 {
            let zero = u8x16::splat(b'0');
            let nine = u8x16::splat(b'9');
            let dot = u8x16::splat(b'.');
            let limit = len - 15;
            while i < limit {
                let b = u8x16::from_slice(&haystack[i..i + 16]);
                let digit = b.simd_ge(zero) & b.simd_le(nine);
                let d = b.simd_eq(dot);
                let class = digit | d;
                let mask = class.to_bitmask() as u16;
                if mask != 0xFFFF {
                    return i + (!mask).trailing_zeros() as usize;
                }
                i += 16;
            }
        }
        scalar_skip_num_cont(haystack, i)
    }

    #[inline]
    fn scalar_find(needle: u8, haystack: &[u8], start: usize) -> Option<usize> {
        let mut i = start;
        while i < haystack.len() {
            if haystack[i] == needle {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    #[inline]
    fn scalar_skip_ident_cont(haystack: &[u8], start: usize) -> usize {
        let mut i = start;
        while i < haystack.len() {
            let b = haystack[i];
            if !(b.is_ascii_alphanumeric() || b == b'_') {
                return i;
            }
            i += 1;
        }
        i
    }

    #[inline]
    fn scalar_skip_num_cont(haystack: &[u8], start: usize) -> usize {
        let mut i = start;
        while i < haystack.len() {
            let b = haystack[i];
            if !(b.is_ascii_digit() || b == b'.') {
                return i;
            }
            i += 1;
        }
        i
    }
}

#[cfg(test)]
mod tests {
    use super::{find_byte, skip_ident_cont, skip_num_cont};

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

    #[test]
    fn skip_ident_cont_stops_at_boundary() {
        assert_eq!(skip_ident_cont(b"abc_DEF123 x"), 10);
        assert_eq!(skip_ident_cont(b"abc_DEF123"), 10);
        assert_eq!(skip_ident_cont(b" abc"), 0);
        assert_eq!(skip_ident_cont(b""), 0);
    }

    #[test]
    fn skip_ident_cont_crosses_simd_boundary() {
        // 32 ident-cont bytes then a space; verifies we keep going past a
        // full all-match block into the next block where the boundary lies.
        let input = b"abcdefghijklmnopqrstuvwxyz012345 X";
        assert_eq!(skip_ident_cont(input), 32);
    }

    #[test]
    fn skip_ident_cont_all_match_long() {
        let input = b"abcdefghijklmnopqrstuvwxyzABCDEF";
        assert_eq!(skip_ident_cont(input), input.len());
    }

    #[test]
    fn skip_ident_cont_short_input() {
        assert_eq!(skip_ident_cont(b"abc"), 3);
        assert_eq!(skip_ident_cont(b"a c"), 1);
    }

    #[test]
    fn skip_num_cont_stops_at_boundary() {
        assert_eq!(skip_num_cont(b"123.456 x"), 7);
        assert_eq!(skip_num_cont(b"123abc"), 3);
        assert_eq!(skip_num_cont(b""), 0);
    }

    #[test]
    fn skip_num_cont_handles_long_inputs() {
        let input = b"123456789012345678 X";
        assert_eq!(skip_num_cont(input), 18);
        let input = b"1.2.3.4.5.6.7.8.9.0";
        assert_eq!(skip_num_cont(input), input.len());
    }

    #[test]
    fn skip_non_ascii_bytes_are_class_boundary() {
        let mut input = b"abc".to_vec();
        // é (0xC3 0xA9); high-bit bytes must not classify as ident-cont.
        input.extend_from_slice(&[0xC3, 0xA9]);
        input.extend_from_slice(b"xyz0000000000000");
        assert_eq!(skip_ident_cont(&input), 3);
    }
}
