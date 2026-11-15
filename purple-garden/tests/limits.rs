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

fn bytecode_config(opt: usize) -> Config {
    let mut config = Config::default();
    config.opt = opt;
    config.no_jit = true;
    config
}

/// Compiling `source` to bytecode fails with a diagnostic containing `message`, at `at` in the
/// source.
fn assert_bytecode_rejects(source: &str, message: &str, at: &str) {
    for opt in [0, 1] {
        let pg = Pg::new().with_stdlib().config(bytecode_config(opt));
        let err = pg
            .compile(source.as_bytes())
            .err()
            .unwrap_or_else(|| panic!("O{opt}: compiled although past a limit"));
        assert!(err.message.contains(message), "O{opt}: {}", err.message);
        let start = err.primary.span.start;
        assert!(
            source[start..].starts_with(at),
            "O{opt}: reported at `{}`",
            &source[start..(start + 20).min(source.len())]
        );
    }
}

/// `n` statements printing a call's result, a few bytecode instructions each.
fn statements(n: usize) -> String {
    (0..n).map(|i| format!("    io.println(id({i}))\n")).collect()
}

/// Jumps encode a 16 bit pc: a function far into the bytecode may run straight through, but a
/// jump landing past the first 65536 instructions is rejected, as are more values live at once
/// than spill slots and literals larger than an allocation encodes. None panics or wraps.
#[test]
fn bytecode_limits_are_diagnosed() {
    let prelude = "import \"io\"\nfn id(x:Int) Int { x }\n";
    let big = format!(
        "fn big(x:Int) Int {{\n    let y = match {{ x == 0 {{ 1 }} {{ 2 }} }}\n{}    y\n}}\n",
        statements(30_000),
    );
    let branchy = "fn branchy(x:Int) Int { match { x == 3 { 33 } { 44 } } }\n";
    let call = "big(id(0)) + branchy(id(3))";
    // functions are laid out in order, branchy's jump lands past the long one
    let far = format!("{prelude}{big}{branchy}{call}");
    assert_bytecode_rejects(&far, "a jump lands past the first 65536 instructions", "x == 3");

    // before the long function its jumps land early, and the long one runs straight through
    let early = format!("{prelude}{branchy}{big}{call}");
    for opt in [0, 1] {
        let pg = Pg::new().with_stdlib().config(bytecode_config(opt));
        let mut program = pg
            .compile(early.as_bytes())
            .unwrap_or_else(|err| panic!("O{opt}: {err}"));
        assert_eq!(program.run_take::<i64>().unwrap(), 1 + 33, "O{opt}");
    }

    let live = (0..usize::from(u16::MAX) + 64)
        .map(|i| format!("let v{i} = id({i})\n"))
        .collect::<String>();
    let uses = (0..usize::from(u16::MAX) + 64)
        .map(|i| format!("io.println(v{i})\n"))
        .collect::<String>();
    assert_bytecode_rejects(
        &format!("{prelude}fn f() {{\n{live}{uses}}}\nf()"),
        "more than 65535 values live at once",
        // at a use or at the function, depending on which runs out first
        "",
    );

    let array = |n: usize| format!("{prelude}let a = [{}]\na.[{}]", list(n, |i| format!("id({i})")), n - 1);
    let fits = array(8190);
    let mut program = Pg::new()
        .with_stdlib()
        .config(bytecode_config(1))
        .compile(fits.as_bytes())
        .unwrap();
    assert_eq!(program.run_take::<i64>().unwrap(), 8189);
    assert_bytecode_rejects(&array(8191), "an array literal holds at most 8190 words", "[id(0)");
}

/// Constant folding runs pure builtins in the compiler: a chain doubling a string stops folding
/// once the constant is no longer small, instead of building 2^40 bytes at compile time.
#[test]
fn folding_doesnt_build_large_constants() {
    let lets = (0..40)
        .map(|i| format!("let s{} = str.concat(s{i} s{i})\n", i + 1))
        .collect::<String>();
    let source = format!("import \"str\"\nlet s0 = \"ab\"\n{lets}str.len(s40)");
    for opt in 1..=3 {
        let mut config = Config::default();
        config.opt = opt;
        let started = std::time::Instant::now();
        Pg::new()
            .with_stdlib()
            .config(config)
            .compile(source.as_bytes())
            .unwrap_or_else(|err| panic!("O{opt}: {err}"));
        assert!(started.elapsed().as_secs() < 5, "O{opt} took {:?}", started.elapsed());
    }
}

/// `str.repeat` repeats nothing for a count up to 0 and traps once its result can't be a string,
/// rather than aborting on a capacity overflow or allocating terabytes.
#[test]
fn repeat_counts_are_bounded() {
    let prelude = "import \"str\"\nfn id(x:Int) Int { x }\n";
    assert_runs(&format!("{prelude}str.len(str.repeat(\"ab\" id(3)))"), 6);
    assert_runs(&format!("{prelude}str.len(str.repeat(\"ab\" -1))"), 0);
    assert_runs(&format!("{prelude}str.len(str.repeat(\"ab\" id(-4)))"), 0);
    for count in ["id(1099511627776)", "4611686018427387904"] {
        for config in configs() {
            let label = format!("{count} O{} no_jit={}", config.opt, config.no_jit);
            let source = format!("{prelude}str.len(str.repeat(\"ab\" {count}))");
            let pg = Pg::new().with_stdlib().config(config);
            let mut program = pg.compile(source.as_bytes()).unwrap_or_else(|err| panic!("{label}: {err}"));
            let err = program.run_take::<i64>().expect_err(&label);
            assert!(
                matches!(err, purple_garden::embed::Anomaly::AllocationFailed { .. }),
                "{label}: {err}"
            );
        }
    }
}
