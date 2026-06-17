use purple_garden_frontend::{
    ast::Ast,
    diagnostic::Diagnostic,
    lex::Lexer,
    parser::Parser,
    typecheck::{TypecheckOutput, Typechecker},
};
use purple_garden_runtime::Pkg;
use std::path::{Path, PathBuf};

pub(crate) struct FrontendAnalysis<'a, 'src> {
    pub(crate) ast: Option<&'a Ast<'src>>,
    pub(crate) typecheck: Option<&'a TypecheckOutput<'src>>,
    pub(crate) diagnostics: &'a [Diagnostic],
    pub(crate) source: &'src [u8],
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
            source,
        });
    };

    let typecheck = Typechecker::new(&ast).with_libs(libs).check();
    diagnostics.extend(typecheck.diagnostics.iter().cloned());

    f(FrontendAnalysis {
        ast: Some(&ast),
        typecheck: Some(&typecheck),
        diagnostics: &diagnostics,
        source,
    })
}

pub(crate) fn analyze_path<R>(
    source_path: &Path,
    source: &[u8],
    libs: Vec<&Pkg>,
    f: impl for<'a> FnOnce(FrontendAnalysis<'a, '_>) -> R,
) -> R {
    let Some(extern_path) = find_extern_garden(source_path) else {
        return analyze(source, libs, f);
    };

    let Ok(extern_source) = std::fs::read(&extern_path) else {
        return analyze(source, libs, f);
    };

    let mut combined = Vec::with_capacity(source.len() + extern_source.len() + 1);
    combined.extend_from_slice(source);
    if !combined.ends_with(b"\n") {
        combined.push(b'\n');
    }
    combined.extend_from_slice(&extern_source);

    analyze(&combined, libs, f)
}

fn find_extern_garden(source_path: &Path) -> Option<PathBuf> {
    let source_path = source_path
        .canonicalize()
        .unwrap_or_else(|_| source_path.to_owned());
    let mut dir = source_path.parent()?;

    loop {
        let candidate = dir.join("extern.garden");
        let candidate_canonical = candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.clone());
        if candidate_canonical != source_path && candidate.is_file() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}
