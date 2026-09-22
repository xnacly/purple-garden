pub use self::math::PACKAGE;

#[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
/// Package math implements scalar numeric helpers.
// This module is the public `math` package namespace; its file name matches it.
#[allow(clippy::module_inception)]
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

    /// The greatest common divisor of integers a and b, at least one of which is nonzero, is the
    /// greatest positive integer d such that d is a divisor of both a and b
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "math"
    ///
    /// math.gcd(48 18)
    /// ```
    ///
    /// Implemented as [Binary GCD algorithm](https://en.wikipedia.org/wiki/Binary_GCD_algorithm)
    ///
    /// Traps for u < 0 || v < 0
    #[pg_fn(unsafe)]
    pub fn gcd(vm: &mut purple_garden_runtime::Vm, mut u: i64, mut v: i64) -> i64 {
        if u < 0 || v < 0 {
            // TODO: return type should be Option<Int> / Option<i64>, since gcd is not defined for
            // negative inputs, since we are currently missing optionals due to missing monomorphic
            // generics in the stdlib, we therefore trap
            vm.trap(purple_garden_runtime::Anomaly::Msg {
                msg: "gcd: undefined for u < 0 || v < 0",
                pc: vm.pc,
            })
        }

        // gcd(n, 0) = gcd(0, n) = n
        if u == 0 {
            return v;
        } else if v == 0 {
            return u;
        }

        // Using identities 2 and 3:
        let i = u.trailing_zeros();
        u >>= i;
        let j = v.trailing_zeros();
        v >>= j;
        let k = i.min(j);

        loop {
            // Swap if necessary so u <= v
            if u > v {
                (u, v) = (v, u);
            }

            // Identity 4
            v -= u;
            // v is now even

            if v == 0 {
                // Identity 1
                // The shift by k is necessary to add back the 2 power k
                return u << k;
            }

            // Identity 3
            v >>= v.trailing_zeros();
        }
    }
}
