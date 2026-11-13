pub use self::bit::PACKAGE;

#[purple_garden_macros::pg_pkg(runtime = purple_garden_runtime)]
/// Package bit implements bitwise operations on two's complement integers.
// This module is the public `bit` package namespace; its file name matches it.
#[allow(clippy::module_inception)]
pub mod bit {
    /// Returns the bitwise and of `a` and `b`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "bit"
    ///
    /// bit.and(12 10)
    /// ```
    #[pg_fn(pure, intrinsic = Bin(IAnd))]
    pub fn and(a: i64, b: i64) -> i64 {
        a & b
    }

    /// Returns the bitwise or of `a` and `b`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "bit"
    ///
    /// bit.or(12 10)
    /// ```
    #[pg_fn(pure, intrinsic = Bin(IOr))]
    pub fn or(a: i64, b: i64) -> i64 {
        a | b
    }

    /// Returns the bitwise exclusive or of `a` and `b`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "bit"
    ///
    /// bit.xor(12 10)
    /// ```
    #[pg_fn(pure, intrinsic = Bin(IXor))]
    pub fn xor(a: i64, b: i64) -> i64 {
        a ^ b
    }

    /// Returns the bitwise complement of `n`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "bit"
    ///
    /// bit.not(0)
    /// ```
    #[pg_fn(pure, intrinsic = Un(INot))]
    pub fn not(n: i64) -> i64 {
        !n
    }

    /// Shifts `n` left by `by` mod 64 bits.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "bit"
    ///
    /// bit.shl(1 4)
    /// ```
    #[pg_fn(pure, intrinsic = Bin(IShl))]
    pub fn shl(n: i64, by: i64) -> i64 {
        n.wrapping_shl(by as u32)
    }

    /// Shifts `n` right by `by` mod 64 bits, filling with the sign bit.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "bit"
    ///
    /// bit.shr(-16 2)
    /// ```
    #[pg_fn(pure, intrinsic = Bin(IShr))]
    pub fn shr(n: i64, by: i64) -> i64 {
        n.wrapping_shr(by as u32)
    }

    /// Shifts `n` right by `by` mod 64 bits, filling with zeros.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "bit"
    ///
    /// bit.ushr(-1 60)
    /// ```
    #[pg_fn(pure, intrinsic = Bin(IUShr))]
    pub fn ushr(n: i64, by: i64) -> i64 {
        (n as u64).wrapping_shr(by as u32) as i64
    }

    /// Returns the number of set bits in `n`.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "bit"
    ///
    /// bit.popcount(255)
    /// ```
    #[pg_fn(pure, intrinsic = Un(IPopcnt))]
    pub fn popcount(n: i64) -> i64 {
        i64::from(n.count_ones())
    }

    /// Returns the number of trailing zero bits in `n`, 64 for 0.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "bit"
    ///
    /// bit.ctz(8)
    /// ```
    #[pg_fn(pure, intrinsic = Un(ICtz))]
    pub fn ctz(n: i64) -> i64 {
        i64::from(n.trailing_zeros())
    }

    /// Returns the number of leading zero bits in `n`, 64 for 0.
    ///
    /// ## Examples
    ///
    /// ```garden
    /// import "bit"
    ///
    /// bit.clz(1)
    /// ```
    #[pg_fn(pure, intrinsic = Un(IClz))]
    pub fn clz(n: i64) -> i64 {
        i64::from(n.leading_zeros())
    }
}
