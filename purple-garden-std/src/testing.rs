pub use self::testing::PACKAGE;

#[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
/// Package testing includes helpers for runtime assertions.
// This module is the public `testing` package namespace; its file name matches it.
#[allow(clippy::module_inception)]
pub mod testing {
    /// Asserts that `condition` is true.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "testing"
    ///
    /// testing.assert(1 + 1 == 2)
    /// ```
    #[pg_fn(intrinsic = Assert)]
    pub fn assert(condition: bool) {
        let _ = condition;
        unreachable!("lowered to Instr::Assert")
    }
}
