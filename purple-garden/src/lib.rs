#[cfg(not(all(
    any(target_os = "linux", target_os = "macos"),
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
compile_error!("purple-garden currently supports only Linux or macOS on x86_64 or aarch64");

use purple_garden_bc as bc;
use purple_garden_frontend::{err::PgError, lex, lower, parser};
use purple_garden_runtime::{Anomaly, BuiltinFn, DebugInfo};
pub use purple_garden_shared::config;

pub use purple_garden_macros::{FromVm, IntoVm, PgType, pg_fn, pg_pkg};
pub use purple_garden_runtime::{Fn, FromVm, IntoVm, PgType, Pkg, Type, Value, Vm, VmConfig};
pub use purple_garden_std::{STD, resolve_pkg};

pub mod gc;
pub mod help;
pub mod input;

type JitFn = purple_garden_jit::JitFn;

#[derive(Debug)]
pub struct Pg<'pg> {
    config: config::Config,
    libs: Vec<&'pg Pkg>,
}

impl<'pg> Pg<'pg> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: config::Config::default(),
            libs: Vec::new(),
        }
    }

    #[must_use]
    pub fn config(mut self, config: config::Config) -> Self {
        self.config = config;
        self
    }

    #[must_use]
    pub fn with_stdlib(self) -> Self {
        self
    }

    #[must_use]
    pub fn with_lib(mut self, lib: &'pg Pkg) -> Self {
        self.libs.push(lib);
        self
    }

    pub fn compile(&self, input: &[u8]) -> Result<Program, PgError> {
        compile(&self.config, input, &self.libs)
    }
}

impl Default for Pg<'_> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct Program {
    pub vm: Vm,
    pub entry: usize,
    pub syscalls: Vec<BuiltinFn>,
    pub debug: DebugInfo,
    pub jit: Vec<JitFn>,
}

impl Program {
    #[must_use]
    #[doc(hidden)]
    pub fn from_vm(vm: Vm, syscalls: Vec<BuiltinFn>, debug: DebugInfo) -> Self {
        let entry = vm.pc;
        Self {
            vm,
            entry,
            syscalls,
            debug,
            jit: Vec::new(),
        }
    }

    pub fn push_jit(&mut self, jit: JitFn) -> u16 {
        let idx = self.syscalls.len() as u16;
        self.syscalls.push(jit.entry());
        self.jit.push(jit);
        idx
    }

    pub fn run(&mut self) -> Result<(), Anomaly> {
        self.vm.run(&self.syscalls)
    }

    /// Run the program and decode the entry return value from `r0`.
    ///
    /// Top-level scripts return their final value-producing expression. If the
    /// script has no final value, use [`Program::run`] instead.
    pub fn run_take<'vm, T: FromVm<'vm>>(&'vm mut self) -> Result<T, Anomaly> {
        self.vm.run(&self.syscalls)?;
        Ok(T::from_vm(&self.vm, 0))
    }
}

fn compile<'e>(
    config: &'e config::Config,
    input: &'e [u8],
    libs: &[&'e Pkg],
) -> Result<Program, PgError> {
    let lexer = lex::Lexer::new(input);
    let ast = parser::Parser::new(lexer)?.parse()?;

    let mut ir = lower::Lower::new().with_libs(libs.to_vec()).ir_from(&ast)?;
    if config.opt >= 1 {
        purple_garden_opt::ir(&mut ir);
    }

    let mut cc = bc::Cc::new();
    let native_pages = cc.compile(config, &ir).map_err(|msg| PgError {
        msg,
        start: 0,
        len: 0,
    })?;
    if config.opt >= 1 {
        purple_garden_opt::bc(&mut cc.buf);
        cc.compact_nops();
    }

    let (vm, syscalls, debug) = cc.finalize(VmConfig {
        backtrace: config.backtrace,
    });
    let mut program = Program::from_vm(vm, syscalls, debug);
    if !config.no_jit {
        program.jit = native_pages;
    }
    Ok(program)
}
