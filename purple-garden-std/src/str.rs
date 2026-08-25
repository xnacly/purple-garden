pub use self::str::PACKAGE;

#[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
/// Package str implements functions for creating, inspecting, and combining strings.
// This module is the public `str` package namespace; its file name matches it.
#[allow(clippy::module_inception)]
pub mod str {
    use purple_garden_runtime::{PgType, Type, Value, Vm};

    pub struct RawString(Value);

    impl PgType for RawString {
        const TYPE: Type<'static> = Type::Str;
    }

    impl purple_garden_runtime::FromVm<'_> for RawString {
        fn from_vm(_: &Vm, value: Value) -> Self {
            Self(value)
        }
    }

    impl purple_garden_runtime::IntoVm for RawString {
        fn into_vm(self, _: &mut Vm) -> Value {
            self.0
        }
    }

    /// Reports whether `needle` appears in `hay`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "str"
    ///
    /// str.contains("purple garden" "garden")
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
    /// import "str"
    ///
    /// str.repeat("ha" 3)
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
    /// import "str"
    ///
    /// let greeting = str.concat("hello " "garden")
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
    /// import "str"
    ///
    /// str.len("garden")
    /// ```
    #[purple_garden_macros::pg_fn(pure)]
    pub fn len(s: &str) -> i64 {
        s.len() as i64
    }

    /// Returns the byte at `index` as an `Int`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "str"
    ///
    /// str.get("garden" 0)
    /// ```
    pub fn get(s: &str, index: i64) -> Result<i64, &'static str> {
        let index = usize::try_from(index).map_err(|_| "str.get: negative index")?;
        s.as_bytes()
            .get(index)
            .map(|&byte| i64::from(byte))
            .ok_or("str.get: index out of bounds")
    }

    /// Returns the substring from byte offset `start` up to but not including `end`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "str"
    ///
    /// str.slice("garden" 0 3)
    /// ```
    #[purple_garden_macros::pg_fn(unsafe)]
    pub fn slice(
        vm: &mut purple_garden_runtime::Vm,
        s: RawString,
        start: i64,
        end: i64,
    ) -> Result<RawString, &'static str> {
        let s = s.0.as_str();
        let start = usize::try_from(start).map_err(|_| "str.slice: negative start")?;
        let end = usize::try_from(end).map_err(|_| "str.slice: negative end")?;
        if start > end || end > s.len() {
            return Err("str.slice: range out of bounds");
        }
        if !s.is_char_boundary(start) || !s.is_char_boundary(end) {
            return Err("str.slice: range is not on utf-8 boundaries");
        }

        Ok(RawString(vm.new_string_from_str(&s[start..end])))
    }

    /// Converts an `Int` to `Str`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "str"
    ///
    /// str.from(42)
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
    /// import "str"
    ///
    /// str.from(3.1415)
    /// ```
    #[pg_fn(pure, specialises = "from")]
    pub fn from_double(d: f64) -> String {
        d.to_string()
    }
}
