//! The arithmetic edge cases of the language on every backend, each in three shapes:
//!
//! - runtime: both operands are arguments, called from Rust (`Bin` in the VM and the JIT)
//! - immediate: the right operand is a literal (`BinImm` once `imm_fold` ran, shifts for powers
//!   of two)
//! - folded: both are literals, `const_fold` computes the result at compile time from -O1 on and
//!   must neither panic nor disagree with the runtime
//!
//! `str.repeat`'s bounds are tested in `purple-garden/tests/limits.rs`.
//!
//! The semantics are Rust's wrapping ones: `+ - *` wrap, `i64::MIN / -1` is `i64::MIN` and
//! `i64::MIN % -1` is 0, only a zero divisor traps. `Double as Int` saturates, NaN is 0.

use purple_garden::{CallError, Pg, Program};
use purple_garden_fuzz::{
    generate::{double_lit, int_lit},
    oracle::{BACKENDS, Backend, Outcome},
};

const INTS: &[i64] = &[
    0,
    1,
    -1,
    2,
    -2,
    7,
    -7,
    32767,
    -32768,
    32768,
    i32::MAX as i64,
    i32::MIN as i64,
    1 << 32,
    1 << 62,
    i64::MAX,
    i64::MIN,
    i64::MIN + 1,
];

const DOUBLES: &[f64] = &[
    f64::NAN,
    f64::INFINITY,
    f64::NEG_INFINITY,
    0.0,
    -0.0,
    0.5,
    -0.5,
    -1.5,
    9.2e18,
    9223372036854775807.0,
    -9223372036854775808.0,
    -9.3e18,
    1e300,
    -1e300,
    f64::MIN_POSITIVE,
];

const DIV_ZERO: &str = "Division by zero";

fn int_op(op: &str, a: i64, b: i64) -> Result<i64, String> {
    Ok(match op {
        "+" => a.wrapping_add(b),
        "-" => a.wrapping_sub(b),
        "*" => a.wrapping_mul(b),
        "/" if b == 0 => return Err(DIV_ZERO.into()),
        "/" => a.wrapping_div(b),
        "%" if b == 0 => return Err(DIV_ZERO.into()),
        "%" => a.wrapping_rem(b),
        "<" => (a < b).into(),
        ">" => (a > b).into(),
        "<=" => (a <= b).into(),
        ">=" => (a >= b).into(),
        "==" => (a == b).into(),
        "!=" => (a != b).into(),
        _ => unreachable!("{op}"),
    })
}

const ARITH: &[&str] = &["+", "-", "*", "/", "%"];
const CMP: &[&str] = &["<", ">", "<=", ">=", "==", "!="];

/// Comparisons yield a Bool, the programs cast it so every case returns an Int.
fn int_expr(op: &str, a: &str, b: &str) -> String {
    if CMP.contains(&op) {
        format!("(({a} {op} {b}) as Int)")
    } else {
        format!("({a} {op} {b})")
    }
}

fn compile<'s>(src: &'s str, backend: Backend) -> Program<'s> {
    Pg::new()
        .with_stdlib()
        .config(backend.config())
        .compile(src.as_bytes())
        .unwrap_or_else(|d| panic!("{backend} rejected {src}: {d:?}"))
}

fn runtime<T>(result: Result<T, CallError>) -> Result<T, String> {
    result.map_err(|e| match e {
        CallError::Runtime(anomaly) => anomaly.as_str().to_string(),
        other => panic!("signature mismatch: {other}"),
    })
}

fn folded(src: &str, backend: Backend) -> Result<i64, String> {
    match backend.run(src) {
        Outcome::Value(v) => Ok(v),
        Outcome::Anomaly(a) => Err(a),
        Outcome::Rejected(d) => panic!("{backend} rejected {src}: {d}"),
    }
}

#[test]
fn int_ops_on_runtime_operands() {
    for op in ARITH.iter().chain(CMP) {
        let src = format!("fn op(a:Int b:Int) Int {{ {} }}", int_expr(op, "a", "b"));
        for backend in BACKENDS {
            let mut program = compile(&src, backend);
            let f = program
                .function::<(i64, i64), i64>("op")
                .expect("op is declared");
            for &a in INTS {
                for &b in INTS {
                    assert_eq!(
                        runtime(program.call(&f, (a, b))),
                        int_op(op, a, b),
                        "{backend}: {a} {op} {b}"
                    );
                }
            }
        }
    }
}

#[test]
fn int_ops_on_an_immediate() {
    for op in ARITH.iter().chain(CMP) {
        for &b in INTS.iter().chain(&[3, 4, 8, -8, 1024, 1 << 31]) {
            let src = format!("fn op(a:Int) Int {{ {} }}", int_expr(op, "a", &int_lit(b)));
            for backend in BACKENDS {
                let mut program = compile(&src, backend);
                let f = program
                    .function::<(i64,), i64>("op")
                    .expect("op is declared");
                for &a in INTS {
                    assert_eq!(
                        runtime(program.call(&f, (a,))),
                        int_op(op, a, b),
                        "{backend}: {a} {op} {b}"
                    );
                }
            }
        }
    }
}

#[test]
fn int_ops_folded_at_compile_time() {
    for op in ARITH.iter().chain(CMP) {
        for &a in INTS {
            for &b in INTS {
                let src = int_expr(op, &int_lit(a), &int_lit(b));
                for backend in BACKENDS {
                    assert_eq!(folded(&src, backend), int_op(op, a, b), "{backend}: {src}");
                }
            }
        }
    }
}

#[test]
fn negation_and_abs_wrap() {
    let src = "import \"math\"\nfn neg(a:Int) Int { -a }\nfn abs(a:Int) Int { math.abs(a) }";
    for backend in BACKENDS {
        let mut program = compile(src, backend);
        let neg = program.function::<(i64,), i64>("neg").expect("declared");
        let abs = program.function::<(i64,), i64>("abs").expect("declared");
        for &a in INTS {
            assert_eq!(
                runtime(program.call(&neg, (a,))),
                Ok(a.wrapping_neg()),
                "{backend}: -{a}"
            );
            assert_eq!(
                runtime(program.call(&abs, (a,))),
                Ok(a.wrapping_abs()),
                "{backend}: abs {a}"
            );
        }
        for &a in INTS {
            let lit = int_lit(a);
            for (src, want) in [
                (format!("-{lit}"), a.wrapping_neg()),
                (
                    format!("import \"math\"\nmath.abs({lit})"),
                    a.wrapping_abs(),
                ),
            ] {
                assert_eq!(folded(&src, backend), Ok(want), "{backend}: {src}");
            }
        }
    }
}

/// The `bit` package's shifts take their count mod 64, also for negative and huge counts.
#[test]
fn shifts_take_the_count_mod_64() {
    const COUNTS: &[i64] = &[0, 1, 31, 63, 64, 65, 128, -1, -63, -64, i64::MIN, i64::MAX];
    let shifts: [(&str, fn(i64, i64) -> i64); 3] = [
        ("shl", |n, by| n.wrapping_shl(by as u32)),
        ("shr", |n, by| n.wrapping_shr(by as u32)),
        ("ushr", |n, by| (n as u64).wrapping_shr(by as u32) as i64),
    ];
    for (name, want) in shifts {
        let src = format!("import \"bit\"\nfn op(n:Int by:Int) Int {{ bit.{name}(n by) }}");
        for backend in BACKENDS {
            let mut program = compile(&src, backend);
            let f = program.function::<(i64, i64), i64>("op").expect("declared");
            for &n in INTS {
                for &by in COUNTS {
                    assert_eq!(
                        runtime(program.call(&f, (n, by))),
                        Ok(want(n, by)),
                        "{backend}: bit.{name}({n} {by})"
                    );
                    let folded_src =
                        format!("import \"bit\"\nbit.{name}({} {})", int_lit(n), int_lit(by));
                    assert_eq!(
                        folded(&folded_src, backend),
                        Ok(want(n, by)),
                        "{backend}: {folded_src}"
                    );
                }
            }
        }
    }
}

/// `Double as Int` saturates like Rust's `as`: NaN is 0, out of range is the closest bound.
#[test]
fn double_to_int_saturates() {
    let src = "fn cast(d:Double) Int { d as Int }";
    for backend in BACKENDS {
        let mut program = compile(src, backend);
        let cast = program.function::<(f64,), i64>("cast").expect("declared");
        for &d in DOUBLES {
            assert_eq!(
                runtime(program.call(&cast, (d,))),
                Ok(d as i64),
                "{backend}: {d} as Int"
            );
            let folded_src = format!("{} as Int", double_lit(d));
            assert_eq!(
                folded(&folded_src, backend),
                Ok(d as i64),
                "{backend}: {folded_src}"
            );
        }
    }
}

#[test]
fn int_to_double_and_bool() {
    let src = "fn double(a:Int) Double { a as Double }\nfn bool(a:Int) Bool { a as Bool }";
    for backend in BACKENDS {
        let mut program = compile(src, backend);
        let double = program.function::<(i64,), f64>("double").expect("declared");
        let bool = program.function::<(i64,), bool>("bool").expect("declared");
        for &a in INTS {
            assert_eq!(
                runtime(program.call(&double, (a,))).map(f64::to_bits),
                Ok((a as f64).to_bits()),
                "{backend}: {a} as Double"
            );
            assert_eq!(
                runtime(program.call(&bool, (a,))),
                Ok(a != 0),
                "{backend}: {a} as Bool"
            );
            let folded_src = format!("({} as Bool) as Int", int_lit(a));
            assert_eq!(
                folded(&folded_src, backend),
                Ok(i64::from(a != 0)),
                "{backend}: {folded_src}"
            );
        }
    }
}

/// `math.clamp` applies `lo` then `hi`, so `hi` wins on inverted bounds, a NaN bound is no bound
/// and a NaN stays NaN. It used to panic on all of these, at runtime and while folding.
#[test]
fn clamp_is_total() {
    let src = "import \"math\"\nfn int(n:Int lo:Int hi:Int) Int { math.clamp(n lo hi) }\n\
               fn double(n:Double lo:Double hi:Double) Double { math.clamp(n lo hi) }";
    let clamp_int = |n: i64, lo: i64, hi: i64| n.max(lo).min(hi);
    for backend in BACKENDS {
        let mut program = compile(src, backend);
        let int = program
            .function::<(i64, i64, i64), i64>("int")
            .expect("declared");
        let double = program
            .function::<(f64, f64, f64), f64>("double")
            .expect("declared");
        for (n, lo, hi) in [
            (5, 10, 0),
            (-5, 10, 0),
            (5, 0, 10),
            (i64::MIN, i64::MAX, i64::MIN),
        ] {
            assert_eq!(
                runtime(program.call(&int, (n, lo, hi))),
                Ok(clamp_int(n, lo, hi)),
                "{backend}: clamp({n} {lo} {hi})"
            );
            let folded_src = format!(
                "import \"math\"\nmath.clamp({} {} {})",
                int_lit(n),
                int_lit(lo),
                int_lit(hi)
            );
            assert_eq!(
                folded(&folded_src, backend),
                Ok(clamp_int(n, lo, hi)),
                "{backend}: {folded_src}"
            );
        }
        for (n, lo, hi, want) in [
            (5.0, 10.0, 0.0, 0.0),
            (0.5, f64::NAN, 1.0, 0.5),
            (2.0, 0.0, f64::NAN, 2.0),
            (f64::NAN, 0.0, 1.0, f64::NAN),
        ] {
            let got = runtime(program.call(&double, (n, lo, hi))).expect("no trap");
            assert_eq!(
                got.to_bits(),
                want.to_bits(),
                "{backend}: clamp({n} {lo} {hi})"
            );
        }
    }
}
