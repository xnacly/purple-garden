//! ANSI SGR palette shared by the IR and bytecode dumps.

use std::io::IsTerminal;

pub const ADDR: &str = "90"; // gray   - pc / addresses
pub const MNEM: &str = "36"; // cyan   - data mnemonics
pub const FLOW: &str = "1;35"; // magenta - control-flow mnemonics
pub const REG: &str = "32"; // green  - registers
pub const IMM: &str = "33"; // yellow - immediates
pub const LABEL: &str = "94"; // blue   - jump targets / symbols
pub const COMMENT: &str = "90"; // gray   - source/value annotations
pub const FUNC: &str = "1;33"; // bold yellow - function headers
pub const BLOCK: &str = "1;94"; // bold blue   - basic-block labels
pub const SECTION: &str = "1"; // bold        - section headers
pub const TYPE: &str = "35"; // magenta - types
pub const DEAD: &str = "1;31"; // bold red - defined but never used

/// Colors only when stdout is a tty and `NO_COLOR` is unset.
#[must_use]
pub fn enabled() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

#[must_use]
pub fn paint(on: bool, code: &str, s: &str) -> String {
    if on {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}
