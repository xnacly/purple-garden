//! Structured differential fuzzing: the input drives the generator of well-typed, terminating
//! programs, which have to give the same checksum or anomaly on the bytecode VM and the JIT at
//! every optimisation level. See `purple-garden-fuzz` for the generator and the oracle.
#![no_main]

use libfuzzer_sys::fuzz_target;
use purple_garden_fuzz::{generate, oracle};

fuzz_target!(|data: &[u8]| {
    let src = generate::program(&mut arbitrary::Unstructured::new(data))
        .expect("generation is total");
    match oracle::check(&src) {
        Ok(oracle::Outcome::Rejected(diagnostic)) => {
            panic!("the generator produced a program the compiler rejects: {diagnostic}\n{src}")
        }
        Ok(_) => {}
        Err(divergence) => panic!("{divergence}\n{src}"),
    }
});
