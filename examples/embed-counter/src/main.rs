#[path = "counter.rs"]
mod counter_pkg;

use purple_garden::Pg;

fn main() {
    // Load the Purple Garden source that will be compiled and executed.
    let input = include_bytes!("counter.garden");

    // Register the embedded package so the compiler can resolve `import "counter"` and `counter.*`
    // references in the script against the Rust implementation below.
    let mut program = Pg::new()
        .with_stdlib()
        .with_lib(&counter_pkg::counter::PACKAGE)
        .compile(input)
        .expect("counter script should compile");

    // Run the script and decode its return value as a borrowed Rust handle.
    // The script returns the `Counter` it created, so the VM result is the
    // same opaque foreign type we defined with `PgType`, `FromVm`, and `IntoVm`.
    let counter = program
        .run_take::<&counter_pkg::Counter>()
        .expect("counter script should run");

    // Call back into the embedded package from Rust to show that the value
    // returned by Purple Garden can still be used by native code.
    counter_pkg::counter::increment(counter);
    assert_eq!(counter_pkg::counter::get(counter), 4);

    // Print the Rust-side view of the foreign value after the round-trip.
    println!("{:#?}", counter);
}
