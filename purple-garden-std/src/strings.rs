pub use self::strings::PACKAGE;

#[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
/// Package strings implements functions for creating, inspecting, and combining strings.
pub mod strings {
    /// Reports whether `needle` appears in `hay`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "strings"
    ///
    /// strings.contains("purple garden" "garden")
    /// ```
    #[purple_garden_macros::pg_fn(pure)]
    pub fn contains(hay: &str, needle: &str) -> bool {
        hay.contains(needle)
    }

    /// Repeats `s` `n` times.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "strings"
    ///
    /// strings.repeat("ha" 3)
    /// ```
    #[purple_garden_macros::pg_fn(pure)]
    pub fn repeat(s: &str, n: i64) -> String {
        s.repeat(n as usize)
    }

    /// Concatenates `a` and `b`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "strings"
    ///
    /// let greeting = strings.concat("hello " "garden")
    /// ```
    #[purple_garden_macros::pg_fn(pure)]
    pub fn concat(a: &str, b: &str) -> String {
        let mut out = String::with_capacity(a.len() + b.len());
        out.push_str(a);
        out.push_str(b);
        out
    }

    /// Returns the length of `s` in bytes.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "strings"
    ///
    /// strings.len("garden")
    /// ```
    #[purple_garden_macros::pg_fn(pure)]
    pub fn len(s: &str) -> i64 {
        s.len() as i64
    }

    /// Converts an `Int` to `Str`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "strings"
    ///
    /// strings.from(42)
    /// ```
    #[pg_fn(pure, specialises = "from")]
    pub fn from_i64(i: i64) -> String {
        i.to_string()
    }

    /// Converts a `Double` to `Str`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "strings"
    ///
    /// strings.from(3.1415)
    /// ```
    #[pg_fn(pure, specialises = "from")]
    pub fn from_double(d: f64) -> String {
        d.to_string()
    }
}
