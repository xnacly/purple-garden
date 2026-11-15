//! Programs at and just past the limits of `purple_garden_frontend::limits`: at a limit they
//! compile and run in the vm and natively at every optimisation level, past it they are rejected
//! with a diagnostic instead of crashing or miscompiling.

use purple_garden::Pg;
use purple_garden_frontend::limits::{MAX_NESTING, MAX_TYPE_PARTS};
use purple_garden_shared::config::Config;

fn configs() -> impl Iterator<Item = Config> {
    (0..=3).flat_map(|opt| {
        [false, true].map(|no_jit| {
            let mut config = Config::default();
            config.opt = opt;
            config.no_jit = no_jit;
            config
        })
    })
}

/// Compile and run `source` in every configuration, expecting `expected`.
fn assert_runs(source: &str, expected: i64) {
    for config in configs() {
        let label = format!("O{} no_jit={}", config.opt, config.no_jit);
        let mut program = Pg::new()
            .with_stdlib()
            .config(config)
            .compile(source.as_bytes())
            .unwrap_or_else(|err| panic!("{label}: {err}"));
        let result = program
            .run_take::<i64>()
            .unwrap_or_else(|err| panic!("{label}: {err}"));
        assert_eq!(result, expected, "{label}");
    }
}

/// Compiling `source` fails with a diagnostic starting with `message`.
fn assert_rejected(source: &str, message: &str) {
    let err = Pg::new()
        .with_stdlib()
        .compile(source.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("compiled although past a limit"));
    assert!(
        err.message.starts_with(message),
        "expected `{message}`, got `{}`",
        err.message
    );
}

fn list(n: usize, item: impl Fn(usize) -> String) -> String {
    (0..n).map(item).collect::<Vec<_>>().join(" ")
}

/// Each shape nests `n` levels of one construct, as a program evaluating to an Int.
const SHAPES: &[(&str, fn(usize) -> String)] = &[
    ("parens", |n| format!("{}1{}", "(".repeat(n), ")".repeat(n))),
    ("negations", |n| format!("{}1 * 0 + 1", "-".repeat(n))),
    ("left chain", |n| format!("0{}", " + 1".repeat(n))),
    ("right chain", |n| {
        format!("{}0{}", "1 + (".repeat(n), ")".repeat(n))
    }),
    ("casts", |n| format!("1{}", " as Double as Int".repeat(n))),
    ("logic", |n| {
        format!("match {{ true{} {{ 1 }} {{ 0 }} }}", " & true".repeat(n))
    }),
    ("arrays", |n| {
        format!("let a = {}1{}\n1", "[".repeat(n), "]".repeat(n))
    }),
    ("indexing", |n| {
        format!("let a = {}1{}\na{}", "[".repeat(n), "]".repeat(n), ".[0]".repeat(n))
    }),
    ("records", |n| {
        format!("let r = {}1{}\nr{}", "{a: ".repeat(n), "}".repeat(n), ".a".repeat(n))
    }),
    ("calls", |n| {
        format!("fn id(x:Int) Int {{ x }}\n{}1{}", "id(".repeat(n), ")".repeat(n))
    }),
    ("matches", |n| {
        format!(
            "fn f(x:Int) Int {{ {}x{} }}\nf(1)",
            "match { x == 0 { 0 } { ".repeat(n),
            " } }".repeat(n)
        )
    }),
    ("functions", |n| {
        let open = (0..n).map(|i| format!("fn f{i}() Int {{ ")).collect::<String>();
        format!("{open}{}\nf0()", "1 }".repeat(n))
    }),
    ("array types", |n| {
        format!(
            "fn f(a:{}Int{}) Int {{ 1 }}\n1",
            "Array<".repeat(n),
            ">".repeat(n)
        )
    }),
    ("record types", |n| {
        format!(
            "fn f(r:{}Int{}) Int {{ 1 }}\n1",
            "Record<a: ".repeat(n),
            ">".repeat(n)
        )
    }),
];

fn compiles(source: &str) -> Result<(), String> {
    let pg = Pg::new();
    match pg.compile(source.as_bytes()) {
        Ok(_) => Ok(()),
        Err(err) => Err(err.message),
    }
}

/// Run `f` on a thread with the stack a spawned thread gets by default, more in debug builds
/// whose frames are many times larger.
fn on_small_stack(f: impl FnOnce() + Send + 'static) {
    let stack = if cfg!(debug_assertions) { 64 << 20 } else { 2 << 20 };
    std::thread::Builder::new()
        .stack_size(stack)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

/// The deepest program of every shape the parser accepts compiles and runs in every
/// configuration on a small stack, one level more is rejected with a diagnostic.
#[test]
fn nesting_up_to_the_limit_compiles_and_runs() {
    on_small_stack(|| {
        for (shape, source) in SHAPES {
            // the deepest accepted n, every shape takes at most 3 levels per n
            let (mut ok, mut rejected) = (1, MAX_NESTING + 1);
            compiles(&source(ok)).unwrap_or_else(|err| panic!("{shape} at 1: {err}"));
            while rejected - ok > 1 {
                let mid = (ok + rejected) / 2;
                match compiles(&source(mid)) {
                    Ok(()) => ok = mid,
                    Err(err) if err.starts_with("Nested deeper") => rejected = mid,
                    Err(err) => panic!("{shape} at {mid}: {err}"),
                }
            }
            assert!(ok >= MAX_NESTING / 3, "{shape} rejected at {rejected}");
            assert!(rejected <= MAX_NESTING, "{shape} accepted {ok} levels");
            let deepest = source(ok);
            for config in configs() {
                let label = format!("{shape} at {ok}, O{} no_jit={}", config.opt, config.no_jit);
                let mut program = Pg::new()
                    .config(config)
                    .compile(deepest.as_bytes())
                    .unwrap_or_else(|err| panic!("{label}: {err}"));
                program
                    .run()
                    .unwrap_or_else(|err| panic!("{label}: {err}"));
            }
        }
    });
}

/// Far past the limit every shape is rejected, early enough to stay fast.
#[test]
fn nesting_far_past_the_limit_is_rejected() {
    on_small_stack(|| {
        for (shape, source) in SHAPES {
            let source = source(100_000);
            let started = std::time::Instant::now();
            let err = compiles(&source).expect_err(shape);
            assert!(err.starts_with("Nested deeper"), "{shape}: {err}");
            assert!(started.elapsed().as_secs() < 5, "{shape} took {:?}", started.elapsed());
        }
    });
}

/// A record of `n` Int fields `f0..`, read back by summing every field.
fn wide_record(n: usize) -> String {
    format!(
        "fn id(x:Int) Int {{ x }}
let r = {{ {} }}
let copy = {{ inner: r }}
copy.inner.f0 + copy.inner.f{} * 1000 + copy.inner.f{} * 1000000",
        list(n, |i| format!("f{i}: id({i})")),
        n / 2,
        n - 1,
    )
}

/// The widest record has [`MAX_TYPE_PARTS`] parts with the one around it, copying it inline
/// works, one field more is rejected wherever the type comes from.
#[test]
fn types_up_to_the_limit_compile_and_run() {
    let n = MAX_TYPE_PARTS - 2;
    let expected = (n / 2) as i64 * 1000 + (n - 1) as i64 * 1_000_000;
    on_small_stack(move || assert_runs(&wide_record(n), expected));
    assert_rejected(&wide_record(n + 1), "Type has more than");
    let fields = list(MAX_TYPE_PARTS, |i| format!("f{i}: Int"));
    assert_rejected(
        &format!("fn f(r: Record<{fields}>) Int {{ 1 }}"),
        "Type has more than",
    );
    assert_rejected(
        &format!("extern \"pkg\" {{ fn f() Array<Record<{fields}>> }}"),
        "Type has more than",
    );
}

/// Types built from bindings grow without their syntax nesting: deeper by one per `let`, or twice
/// as large. Each is rejected once past the limit, without walking the shared parts again.
#[test]
fn types_growing_through_bindings_are_rejected() {
    let chain = |n: usize, wrap: &dyn Fn(&str) -> String| {
        let lets = (0..n)
            .map(|i| format!("let v{} = {}\n", i + 1, wrap(&format!("v{i}"))))
            .collect::<String>();
        format!("import \"opt\"\nlet v0 = 1\n{lets}1")
    };
    for (shape, wrap) in [
        ("arrays", &(|v: &str| format!("[{v}]")) as &dyn Fn(&str) -> String),
        ("records", &|v| format!("{{ a: {v} }}")),
        ("options", &|v| format!("opt.some({v})")),
        ("doubling records", &|v| format!("{{ a: {v} b: {v} }}")),
    ] {
        let started = std::time::Instant::now();
        let fits = chain(if shape == "doubling records" { 6 } else { MAX_TYPE_PARTS - 1 }, wrap);
        Pg::new()
            .with_stdlib()
            .compile(fits.as_bytes())
            .unwrap_or_else(|err| panic!("{shape}: {err}"));
        assert_rejected(&chain(10_000, wrap), "Type has more than");
        assert!(started.elapsed().as_secs() < 5, "{shape} took {:?}", started.elapsed());
    }
}
