#[derive(clap::Parser, Debug)]
#[command(about, long_about=None)]
pub struct Config {
    /// Set optimisation level. Higher levels increase compile time.
    ///
    /// pub 0: Baseline lowering with no optimisation passes.
    ///
    /// pub 1: Local IR and bytecode optimisations:
    ///    constant folding and propagation, arithmetic simplification,
    ///    peephole bytecode cleanup, redundant load elimination.
    ///
    /// pub 2: Global IR optimisations:
    ///    control-flow aware dead code elimination,
    ///    register lifetime minimisation, copy propagation.
    ///
    /// pub 3: Aggressive compile-time optimisations:
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
    /// Dump liveness as <%v>: (<def>,<`last_use`>)
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

impl Config {
    #[must_use]
    pub const fn default() -> Self {
        Config {
            opt: 0,
            dry: false,
            disassemble: 0,
            ast: false,
            ir: false,
            backtrace: false,
            no_std: false,
            no_env: false,
            no_gc: false,
            no_jit: false,
            run: None,
            target: None,
            command: None,
            version: 0,
            liveness: false,
        }
    }
}
