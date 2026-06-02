pub use self::io::PACKAGE;

#[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
/// Package io provides rudimentary I/O primitives,
/// like writing and reading from file descriptors
pub mod io {
    // TODO: let the compiler substitute io.println(x) by static type, then
    // reuse the same path for display-backed foreign and struct printing.
    /// writes s to stdout, with a newline appended
    pub fn println(s: &str) {
        println!("{s}");
    }

    /// writes s to stdout
    pub fn print(s: &str) {
        print!("{s}");
    }
}
