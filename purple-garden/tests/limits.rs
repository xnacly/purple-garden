//! Programs at and just past the limits of `purple_garden_frontend::limits`: at a limit they
//! compile and run in the vm and natively at every optimisation level, past it they are rejected
//! with a diagnostic instead of crashing or miscompiling.

use purple_garden::Pg;
use purple_garden_frontend::limits::MAX_NESTING;
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
