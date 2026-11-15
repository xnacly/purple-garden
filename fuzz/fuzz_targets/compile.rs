//! Raw bytes through the whole compiler, with the safe standard library, at every optimisation
//! level and with and without the JIT, without running: any input compiles or is rejected
//! with a diagnostic, never panics.
#![no_main]

use libfuzzer_sys::fuzz_target;
use purple_garden::Pg;
use purple_garden_fuzz::oracle::BACKENDS;

fuzz_target!(|data: &[u8]| {
    for backend in BACKENDS {
        let _ = Pg::new()
            .with_stdlib()
            .config(backend.config())
            .compile(data);
    }
});
