pub use self::testing::PACKAGE;

#[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
/// Package testing includes helpers for runtime assertions and the likes
pub mod testing {
    /// asserts condition is true
    pub fn assert(condition: bool) -> Result<(), &'static str> {
        if condition {
            Ok(())
        } else {
            Err("test.assert: assertion failed")
        }
    }
}
