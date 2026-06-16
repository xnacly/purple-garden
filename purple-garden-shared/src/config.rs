#[derive(Clone, Debug)]
pub struct Config {
    pub opt: usize,
    pub disassemble: u8,
    pub liveness: bool,
    pub backtrace: bool,
    pub no_std: bool,
    pub no_env: bool,
    pub no_gc: bool,
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
