pub use self::strings::PACKAGE;

#[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
/// Package strings implements functions manipulating strings
pub mod strings {
    /// reports whether needle appears in hay
    #[purple_garden_macros::pg_fn(pure)]
    pub fn contains(hay: &str, needle: &str) -> bool {
        hay.contains(needle)
    }

    /// repeats s n times
    pub fn repeat(s: &str, n: i64) -> String {
        s.repeat(n as usize)
    }

    /// concatenates a and b
    #[purple_garden_macros::pg_fn(pure)]
    pub fn concat(a: &str, b: &str) -> String {
        let mut out = String::with_capacity(a.len() + b.len());
        out.push_str(a);
        out.push_str(b);
        out
    }

    /// returns the length of s in bytes
    #[purple_garden_macros::pg_fn(pure)]
    pub fn len(s: &str) -> i64 {
        s.len() as i64
    }
}
