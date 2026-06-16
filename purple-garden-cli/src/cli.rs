use purple_garden_shared::config::Config;

#[derive(clap::Parser, Debug)]
#[command(about, long_about=None)]
pub struct Cli {
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
    #[arg(short = 'O', default_value_t = 1)]
    pub opt: usize,

    /// Execute the whole pipeline but stop before execution
    #[arg(short = 'd', long)]
    pub dry: bool,

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
    #[arg(short = 'D', long, action = clap::ArgAction::Count)]
    pub disassemble: u8,

    /// Readable abstract syntax tree
    #[arg(short = 'A', long)]
    pub ast: bool,

    /// Readable immediate representation
    #[arg(short = 'I', long)]
    pub ir: bool,

    /// Print typechecker output and stop before lowering.
    ///
    /// -T prints top-level bindings and function signatures.
    ///
    /// -TT prints every typed AST value node.
    #[arg(short = 'T', long, action = clap::ArgAction::Count)]
    pub types: u8,

    /// Dump SSA live intervals with the IR positions that define, use, or pass them.
    #[arg(short = 'L', long)]
    pub liveness: bool,

    /// Generate backtraces for function calls
    ///
    /// Technically a brain child of my interview at apple in which we talked about ways of implementing
    /// backtraces for error display for javascript.
    #[arg(short = 'B', long)]
    pub backtrace: bool,

    /// Limit the standard library to necessities
    #[arg(long)]
    pub no_std: bool,

    /// Skip importing of env variables
    #[arg(long)]
    pub no_env: bool,

    /// Disable garbage collection
    #[arg(long)]
    pub no_gc: bool,

    /// Disable Just In Time compilation
    #[arg(long)]
    pub no_jit: bool,

    /// run a single string passed via this flag instead of a file
    #[arg(short)]
    pub run: Option<String>,

    pub target: Option<String>,

    /// display version information, view more with -VV
    #[arg(short = 'V', action = clap::ArgAction::Count)]
    pub version: u8,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Show documentation for a package or a function
    Doc { pkg_or_function: Option<String> },
    /// An introduction to purple garden
    Intro { topic: Option<String> },
}

impl Cli {
    #[must_use]
    pub fn config(&self) -> Config {
        Config {
            opt: self.opt,
            disassemble: self.disassemble,
            liveness: self.liveness,
            backtrace: self.backtrace,
            no_std: self.no_std,
            no_env: self.no_env,
            no_gc: self.no_gc,
            no_jit: self.no_jit,
        }
    }
}
