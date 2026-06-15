pub use self::io::PACKAGE;

use purple_garden_macros::pg_pkg;

#[pg_pkg(runtime = purple_garden_runtime)]
/// Package io provides rudimentary I/O primitives,
/// like writing and reading from file descriptors
pub mod io {

    /// writes a Str to stdout, with a newline appended
    #[pg_fn(specialises = "println")]
    pub fn println_str(s: &str) {
        println!("{s}");
    }

    /// writes an Int to stdout, with a newline appended
    #[pg_fn(specialises = "println")]
    pub fn println_int(i: i64) {
        println!("{i}");
    }

    /// writes a Double to stdout, with a newline appended
    #[pg_fn(specialises = "println")]
    pub fn println_double(d: f64) {
        println!("{d}");
    }

    /// writes a Str to stdout
    #[pg_fn(specialises = "print")]
    pub fn print_str(s: &str) {
        print!("{s}");
    }

    /// writes an Int to stdout
    #[pg_fn(specialises = "print")]
    pub fn print_int(i: i64) {
        print!("{i}");
    }

    /// writes a Double to stdout
    #[pg_fn(specialises = "print")]
    pub fn print_double(d: f64) {
        print!("{d}");
    }
}
