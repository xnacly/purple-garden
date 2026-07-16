use purple_garden_frontend::{ast::Ast, diagnostic::Diagnostic, lex::Lexer, parser::Parser};
use purple_garden_runtime::Pkg;
use purple_garden_typecheck::{TypecheckOutput, Typechecker};
use std::borrow::Cow;
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
    stdlib: &'src [Pkg],
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

    let typecheck = Typechecker::new(&ast)
        .with_libs(libs)
        .with_stdlib(stdlib)
        .check();
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
    stdlib: &'static [Pkg],
    f: impl for<'a> FnOnce(FrontendAnalysis<'a, '_>) -> R,
) -> R {
    let source = source_with_extern(source_path, source);
    analyze(&source, libs, stdlib, f)
}

pub(crate) fn source_with_extern<'src>(source_path: &Path, source: &'src [u8]) -> Cow<'src, [u8]> {
    let Some(extern_path) = find_extern_garden(source_path) else {
        return Cow::Borrowed(source);
    };

    let Ok(extern_source) = std::fs::read(&extern_path) else {
        return Cow::Borrowed(source);
    };

    let mut combined = Vec::with_capacity(source.len() + extern_source.len() + 1);
    combined.extend_from_slice(source);
    if !combined.ends_with(b"\n") {
        combined.push(b'\n');
    }
    combined.extend_from_slice(&extern_source);

    Cow::Owned(combined)
}

pub(crate) fn find_extern_garden(source_path: &Path) -> Option<PathBuf> {
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

#[cfg(test)]
mod tests {
    use std::{fs, time::SystemTime};

    use super::source_with_extern;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "purple-garden-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn source_with_extern_appends_nearby_signatures() {
        let dir = temp_dir("source-with-extern-appends");
        fs::create_dir(&dir).unwrap();
        let source_path = dir.join("script.garden");
        fs::write(dir.join("extern.garden"), b"extern \"pkg\" {}\n").unwrap();

        let source = source_with_extern(&source_path, b"import \"pkg\"");

        assert_eq!(source.as_ref(), b"import \"pkg\"\nextern \"pkg\" {}\n");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn source_with_extern_ignores_source_file_named_extern_garden() {
        let dir = temp_dir("source-with-extern-ignores-self");
        fs::create_dir(&dir).unwrap();
        let source_path = dir.join("extern.garden");
        fs::write(&source_path, b"extern \"pkg\" {}\n").unwrap();

        let source = source_with_extern(&source_path, b"extern \"pkg\" {}\n");

        assert_eq!(source.as_ref(), b"extern \"pkg\" {}\n");

        fs::remove_dir_all(dir).unwrap();
    }
}
