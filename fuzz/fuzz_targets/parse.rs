//! Raw bytes through the lexer and the parser: any input yields an AST or diagnostics.
#![no_main]

use libfuzzer_sys::fuzz_target;
use purple_garden_allocators::bump::BumpAlloc;
use purple_garden_frontend::{lex, parser};

fuzz_target!(|data: &[u8]| {
    let arena = BumpAlloc::new();
    let scratch = BumpAlloc::new();
    let parse = parser::Parser::new(lex::Lexer::new(data), &arena, &scratch).parse_collect();
    assert!(
        parse.ast.is_some() || !parse.diagnostics.is_empty(),
        "the parser returned neither an AST nor a diagnostic"
    );
});
