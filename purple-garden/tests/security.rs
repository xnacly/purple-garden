//! Source programs that used to corrupt memory. Each one has to be rejected or trap now, on every
//! optimisation level and with and without the JIT.

use purple_garden_shared::config::Config;

fn compile_error(source: &str) -> String {
    let mut messages = Vec::new();
    for opt in 0..=3 {
        for no_jit in [false, true] {
            let mut config = Config::default();
            config.opt = opt;
            config.no_jit = no_jit;
            match purple_garden::Pg::new()
                .with_stdlib()
                .config(config)
                .compile(source.as_bytes())
            {
                Ok(_) => panic!("O{opt} no_jit={no_jit} compiled:\n{source}"),
                Err(diagnostic) => messages.push(diagnostic.message),
            }
        }
    }
    messages.dedup();
    assert_eq!(messages.len(), 1, "{messages:?}");
    messages.pop().unwrap()
}

/// Argument registers past r31 bled into the neighbouring operand of the packed op moving them,
/// so the Str parameter read one of the Int arguments as its pointer: `str.len` dereferenced
/// 0x41414162.
#[test]
fn more_arguments_than_registers_are_rejected() {
    let params = (0..39).map(|i| format!("a{i}:Int ")).collect::<String>();
    let args = (0..39)
        .map(|i| format!("{} ", 0x4141_4141 + i))
        .collect::<String>();
    let source =
        format!("import \"str\"\nfn f({params}s:Str) Int {{ str.len(s) }}\nf({args}\"hello\")\n");
    assert_eq!(
        compile_error(&source),
        "Function takes 40 arguments, at most 30 are supported"
    );
}
