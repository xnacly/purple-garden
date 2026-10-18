pub use self::opt::PACKAGE;

#[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
/// Package opt creates, inspects, unpacks and modifies optionals.
// TODO: opt.some and opt.none should lower to ir nodes instead of builtin calls. opt.none has no
// argument to bind T from, so it needs T from its context before it can exist.
#[allow(clippy::module_inception)]
pub mod opt {
    use purple_garden_runtime::Slot;

    /// Wraps `inner` in an optional.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "opt"
    ///
    /// opt.some(42)
    /// ```
    #[pg_fn(with_slots)]
    pub fn some(inner: Slot<"T">) -> Option<Slot<"T">> {
        Some(inner)
    }

    /// Returns the value `opt` holds, `default` if it is None.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import ("opt" "str")
    ///
    /// opt.or(str.get("garden" 9) 0)
    /// ```
    #[pg_fn(with_slots)]
    pub fn or(opt: Option<Slot<"T">>, default: Slot<"T">) -> Slot<"T"> {
        opt.unwrap_or(default)
    }
}
