#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
compile_error!("purple-garden currently supports only Linux on x86_64 or aarch64");

use purple_garden_bc as bc;
use purple_garden_frontend::{err::PgError, lex, lower, parser};
use purple_garden_runtime::{Anomaly, BuiltinFn, DebugInfo, Vm, VmConfig};
use purple_garden_shared::config;

pub mod gc;
pub mod help;
pub mod input;

type JitFn = purple_garden_jit::JitFn;

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
    pub fn new(config: VmConfig) -> Self {
        Self {
            vm: Vm::new(config),
            entry: 0,
            syscalls: Vec::new(),
            debug: DebugInfo::default(),
            jit: Vec::new(),
        }
    }

    #[must_use]
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
}

/// Create the purple garden vm from the given input.
pub fn new<'e>(config: &'e config::Config, input: &'e [u8]) -> Result<Program, PgError> {
    let lexer = lex::Lexer::new(input);
    let ast = parser::Parser::new(lexer)?.parse()?;

    let (mut ir, pkg_fns) = lower::Lower::new().ir_from(&ast)?;
    if config.opt >= 1 {
        purple_garden_opt::ir(&mut ir);
    }

    let mut cc = bc::Cc::new();
    let native_pages = cc.compile(config, &ir, &pkg_fns).map_err(|msg| PgError {
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
