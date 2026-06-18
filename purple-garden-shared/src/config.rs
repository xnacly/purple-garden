#[derive(Clone, Debug)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct Config {
    /// Set optimisation level. Higher levels increase compile time.
    ///
    /// 0: Baseline lowering with no optimisation passes.
    ///
    /// 1: Local IR and bytecode optimisations:
    ///    constant folding and propagation, arithmetic simplification,
    ///    peephole bytecode cleanup, redundant load elimination.
    ///
    /// 2: Global IR optimisations:
    ///    control-flow aware dead code elimination,
    ///    register lifetime minimisation, copy propagation.
    ///
    /// 3: Aggressive compile-time optimisations:
    ///    function inlining, guarded operator specialisation,
    ///    constant hoisting, aggressive register reuse.
    #[cfg_attr(feature = "cli", arg(short = 'O', default_value_t = 1))]
    pub opt: usize,

    /// Dump generated code.
    ///
    /// -D prints readable bytecode and named native machine code.
    ///
    /// -DD writes a minimal relocatable ELF object to stdout, for instance to be passed to objdump:
    ///
    ///    objdump -d (purple-garden -d -DD script.garden | psub)
    ///
    /// Dumping does not stop execution. Add -d when stdout must contain only
    /// the dump, particularly with -DD.
    #[cfg_attr(feature = "cli", arg(short = 'D', long, action = clap::ArgAction::Count))]
    pub disassemble: u8,

    /// Dump SSA live intervals with the IR positions that define, use, or pass them.
    #[cfg_attr(feature = "cli", arg(short = 'L', long))]
    pub liveness: bool,

    /// Generate backtraces for function calls
    ///
    /// Technically a brain child of my interview at apple in which we talked about ways of implementing
    /// backtraces for error display for javascript.
    #[cfg_attr(feature = "cli", arg(short = 'B', long))]
    pub backtrace: bool,

    /// Limit the standard library to necessities
    #[cfg_attr(feature = "cli", arg(long))]
    pub no_std: bool,

    /// Skip importing of env variables
    #[cfg_attr(feature = "cli", arg(long))]
    pub no_env: bool,

    /// Disable garbage collection
    #[cfg_attr(feature = "cli", arg(long))]
    pub no_gc: bool,

    /// Disable Just In Time compilation
    #[cfg_attr(feature = "cli", arg(long))]
    pub no_jit: bool,
}

impl Config {
    #[must_use]
    pub const fn default() -> Self {
        Config {
            opt: 0,
            disassemble: 0,
            backtrace: false,
            no_std: false,
            no_env: false,
            no_gc: false,
            no_jit: false,
            liveness: false,
        }
    }
}
