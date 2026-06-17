pub use self::io::PACKAGE;

use purple_garden_macros::pg_pkg;

#[pg_pkg(runtime = purple_garden_runtime)]
/// Package io provides rudimentary I/O primitives,
/// like writing and reading from file descriptors.
pub mod io {

    /// Writes a `Str` to stdout, with a newline appended.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "io"
    ///
    /// io.println("hello")
    /// ```
    #[pg_fn(specialises = "println")]
    pub fn println_str(s: &str) {
        println!("{s}");
    }

    /// Writes an `Int` to stdout, with a newline appended.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "io"
    ///
    /// io.println(42)
    /// ```
    #[pg_fn(specialises = "println")]
    pub fn println_int(i: i64) {
        println!("{i}");
    }

    /// Writes a `Double` to stdout, with a newline appended.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "io"
    ///
    /// io.println(3.1415)
    /// ```
    #[pg_fn(specialises = "println")]
    pub fn println_double(d: f64) {
        println!("{d}");
    }

    /// Writes a `Str` to stdout without appending a newline.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "io"
    ///
    /// io.print("progress: ")
    /// io.println(10)
    /// ```
    #[pg_fn(specialises = "print")]
    pub fn print_str(s: &str) {
        print!("{s}");
    }

    /// Writes an `Int` to stdout without appending a newline.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "io"
    ///
    /// io.print(10)
    /// io.println(" steps")
    /// ```
    #[pg_fn(specialises = "print")]
    pub fn print_int(i: i64) {
        print!("{i}");
    }

    /// Writes a `Double` to stdout without appending a newline.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "io"
    ///
    /// io.print(0.5)
    /// io.println(" scale")
    /// ```
    #[pg_fn(specialises = "print")]
    pub fn print_double(d: f64) {
        print!("{d}");
    }
}
