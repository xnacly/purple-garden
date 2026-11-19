//! Generated programs give the same result on every backend, see `purple_garden_fuzz::check`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use purple_garden_fuzz::Program;

fuzz_target!(|program: Program| purple_garden_fuzz::check(&program.0));
