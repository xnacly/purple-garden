pub use self::testing::PACKAGE;

#[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
/// Package testing includes helpers for runtime assertions.
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
    pub fn assert(condition: bool) -> Result<(), &'static str> {
        if condition {
            Ok(())
        } else {
            Err("test.assert: assertion failed")
        }
    }
}
