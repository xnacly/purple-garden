use purple_garden_shared::config::Config;

#[derive(clap::Parser, Debug)]
#[command(about, long_about=None)]
pub struct Cli {
    #[command(flatten)]
    pub config: Config,

    /// Execute the whole pipeline but stop before execution
    #[arg(short = 'd', long)]
    pub dry: bool,

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
    /// Start the language server
    Lsp,
}
