use purple_garden_frontend::{
    ast::Ast,
    diagnostic::Diagnostic,
    lex::Lexer,
    parser::Parser,
    typecheck::{TypecheckOutput, Typechecker},
};
use purple_garden_runtime::Pkg;

pub(crate) struct FrontendAnalysis<'a, 'src> {
    pub(crate) ast: Option<&'a Ast<'src>>,
    pub(crate) typecheck: Option<&'a TypecheckOutput<'src>>,
    pub(crate) diagnostics: &'a [Diagnostic],
}

pub(crate) fn analyze<'src, R>(
    source: &'src [u8],
    libs: Vec<&'src Pkg>,
    f: impl for<'a> FnOnce(FrontendAnalysis<'a, '_>) -> R,
) -> R {
    let parse = Parser::new(Lexer::new(source)).parse_collect();
    let purple_garden_frontend::parser::ParseOutput {
        ast,
        mut diagnostics,
    } = parse;

    let Some(ast) = ast else {
        return f(FrontendAnalysis {
            ast: None,
            typecheck: None,
            diagnostics: &diagnostics,
        });
    };

    let typecheck = Typechecker::new(&ast).with_libs(libs).check();
    diagnostics.extend(typecheck.diagnostics.iter().cloned());

    f(FrontendAnalysis {
        ast: Some(&ast),
        typecheck: Some(&typecheck),
        diagnostics: &diagnostics,
    })
}
