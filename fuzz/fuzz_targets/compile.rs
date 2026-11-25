//! Raw bytes through the whole compiler at every optimisation level, with and without the JIT:
//! any input compiles or is rejected with a diagnostic, never panics.
#![no_main]

use libfuzzer_sys::fuzz_target;
use purple_garden::Pg;

fuzz_target!(|data: &[u8]| {
    for backend in purple_garden_fuzz::backends() {
        let _ = Pg::new().with_stdlib().config(backend.config()).compile(data);
    }
});
