//! Shared by the libFuzzer targets in `fuzz/` and the deterministic seeded tests in `tests/`,
//! so CI without libFuzzer runs the same generator and oracle.
//!
//! - [`generate`]: well-typed, terminating programs ending in an `Int` checksum
//! - [`oracle`]: runs a program on every backend and optimisation level and compares
//! - [`seeded`]: the byte stream for a seed, a stand-in for libFuzzer's input

pub mod generate;
pub mod oracle;

/// Generates the program for `seed`: `len` bytes of a splitmix64 stream fed to
/// [`generate::program`].
#[must_use]
pub fn seeded(seed: u64, len: usize) -> String {
    let mut state = seed;
    let bytes: Vec<u8> = (0..len.div_ceil(8))
        .flat_map(|_| {
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            (z ^ (z >> 31)).to_le_bytes()
        })
        .collect();
    generate::program(&mut arbitrary::Unstructured::new(&bytes)).expect("generation is total")
}
