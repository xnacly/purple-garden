//! Differential fuzzing: generated programs have to give the same result on the bytecode VM and
//! the JIT at every optimisation level. The VM at `-O0` is the reference.

use std::fmt;

use arbitrary::{Arbitrary, Unstructured};
use purple_garden::{Pg, config::Config};

mod generate;

/// A well-typed, terminating program generated from the fuzzer's input, see [`generate`]. Its
/// `Debug` is the source, so `cargo fuzz fmt diff <artifact>` prints the failing program.
pub struct Program(pub String);

impl<'a> Arbitrary<'a> for Program {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        generate::program(u).map(Program)
    }
}

impl fmt::Debug for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An optimisation level, on the bytecode VM or the JIT. The JIT only compiles on x86-64,
/// elsewhere it falls back to the VM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Backend {
    pub opt: usize,
    pub jit: bool,
}

/// Every backend, the reference (the VM at `-O0`) first.
pub fn backends() -> impl Iterator<Item = Backend> {
    (0..8).map(|i| Backend {
        opt: i % 4,
        jit: i >= 4,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Value(i64),
    /// The anomaly's message; its pc differs between backends.
    Anomaly(String),
    /// The first diagnostic.
    Rejected(String),
}

impl Backend {
    #[must_use]
    pub fn config(self) -> Config {
        let mut config = Config::default();
        config.opt = self.opt;
        config.no_jit = !self.jit;
        config
    }

    /// Compiles `src` with the safe standard library and runs it to its final `Int`.
    #[must_use]
    pub fn run(self, src: &str) -> Outcome {
        match Pg::new()
            .with_stdlib()
            .config(self.config())
            .compile(src.as_bytes())
        {
            Err(diagnostic) => Outcome::Rejected(format!("{diagnostic:?}")),
            Ok(mut program) => match program.run_take::<i64>() {
                Ok(v) => Outcome::Value(v),
                Err(anomaly) => Outcome::Anomaly(anomaly.as_str().into()),
            },
        }
    }
}

/// Runs `src` on every backend, panics with the first one disagreeing with the reference. A
/// generated program the reference rejects is a generator bug and panics too.
pub fn check(src: &str) {
    let mut backends = backends();
    let reference = backends.next().expect("the reference");
    let expected = reference.run(src);
    if let Outcome::Rejected(diagnostic) = &expected {
        panic!("the generator produced a program the compiler rejects: {diagnostic}");
    }
    for backend in backends {
        assert_eq!(backend.run(src), expected, "{backend:?} disagrees with {reference:?}");
    }
}
