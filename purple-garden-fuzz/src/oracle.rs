//! Differential oracle: the bytecode VM without optimisations is the reference, every other
//! backend and optimisation level has to agree with it on the program's value or on the kind of
//! anomaly that stopped it.

use std::fmt;

use purple_garden::{Pg, config::Config};

/// One way of compiling and running a program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Backend {
    pub opt: usize,
    pub jit: bool,
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "O{} {}", self.opt, if self.jit { "jit" } else { "vm" })
    }
}

/// The reference every other backend is compared to.
pub const REFERENCE: Backend = Backend { opt: 0, jit: false };

/// Every optimisation level on both the bytecode VM and the JIT, reference first. The JIT only
/// compiles on x86-64, elsewhere its rows run the VM again.
pub const BACKENDS: [Backend; 8] = [
    REFERENCE,
    Backend { opt: 1, jit: false },
    Backend { opt: 2, jit: false },
    Backend { opt: 3, jit: false },
    Backend { opt: 0, jit: true },
    Backend { opt: 1, jit: true },
    Backend { opt: 2, jit: true },
    Backend { opt: 3, jit: true },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Value(i64),
    /// The anomaly's message, its pc differs between backends.
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

#[derive(Debug)]
pub struct Divergence {
    pub backend: Backend,
    pub expected: Outcome,
    pub actual: Outcome,
}

impl fmt::Display for Divergence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} gave {:?}, {REFERENCE} gave {:?}",
            self.backend, self.actual, self.expected
        )
    }
}

/// Runs `src` on every backend in [`BACKENDS`], returns the reference's outcome or the first
/// backend disagreeing with it.
///
/// # Errors
///
/// The first [`Divergence`] from the reference.
pub fn check(src: &str) -> Result<Outcome, Divergence> {
    let expected = REFERENCE.run(src);
    for backend in &BACKENDS[1..] {
        let actual = backend.run(src);
        if actual != expected {
            return Err(Divergence {
                backend: *backend,
                expected,
                actual,
            });
        }
    }
    Ok(expected)
}
