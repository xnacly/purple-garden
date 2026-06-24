pub use self::math::PACKAGE;

#[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
/// Package math implements scalar numeric helpers.
pub mod math {
    /// Returns the absolute value of `n`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "math"
    ///
    /// math.abs(-42)
    /// ```
    #[pg_fn(pure, specialises = "abs")]
    pub fn abs_i64(n: i64) -> i64 {
        n.abs()
    }

    /// Returns the absolute value of `n`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "math"
    ///
    /// math.abs(-3.5)
    /// ```
    #[pg_fn(pure, specialises = "abs")]
    pub fn abs_double(n: f64) -> f64 {
        n.abs()
    }

    /// Returns the smaller of `a` and `b`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "math"
    ///
    /// math.min(10 20)
    /// ```
    #[pg_fn(pure, specialises = "min")]
    pub fn min_i64(a: i64, b: i64) -> i64 {
        a.min(b)
    }

    /// Returns the smaller of `a` and `b`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "math"
    ///
    /// math.min(10.5 20.25)
    /// ```
    #[pg_fn(pure, specialises = "min")]
    pub fn min_double(a: f64, b: f64) -> f64 {
        a.min(b)
    }

    /// Returns the larger of `a` and `b`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "math"
    ///
    /// math.max(10 20)
    /// ```
    #[pg_fn(pure, specialises = "max")]
    pub fn max_i64(a: i64, b: i64) -> i64 {
        a.max(b)
    }

    /// Returns the larger of `a` and `b`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "math"
    ///
    /// math.max(10.5 20.25)
    /// ```
    #[pg_fn(pure, specialises = "max")]
    pub fn max_double(a: f64, b: f64) -> f64 {
        a.max(b)
    }

    /// Restricts `n` to the inclusive range `lo` to `hi`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "math"
    ///
    /// math.clamp(300 0 255)
    /// ```
    #[pg_fn(pure, specialises = "clamp")]
    pub fn clamp_i64(n: i64, lo: i64, hi: i64) -> i64 {
        n.clamp(lo, hi)
    }

    /// Restricts `n` to the inclusive range `lo` to `hi`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "math"
    ///
    /// math.clamp(1.5 0.0 1.0)
    /// ```
    #[pg_fn(pure, specialises = "clamp")]
    pub fn clamp_double(n: f64, lo: f64, hi: f64) -> f64 {
        n.clamp(lo, hi)
    }

    /// Returns `n` unchanged.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "math"
    ///
    /// math.round(42)
    /// ```
    #[pg_fn(pure, specialises = "round")]
    pub fn round_i64(n: i64) -> i64 {
        n
    }

    /// Returns the nearest integer value as a `Double`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "math"
    ///
    /// math.round(3.5)
    /// ```
    #[pg_fn(pure, specialises = "round")]
    pub fn round_double(n: f64) -> f64 {
        n.round()
    }
}
