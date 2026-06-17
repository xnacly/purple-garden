use purple_garden_std::{self as pstd, Pkg};

#[must_use]
pub(crate) fn command(query: &str) -> String {
    format!("purple-garden doc {query}")
}

#[must_use]
pub(crate) fn language_doc(
    query: &str,
) -> Option<&'static purple_garden_frontend::lex::KeywordDoc> {
    purple_garden_frontend::lex::keyword_doc(query)
}

#[must_use]
pub(crate) fn type_doc(query: &str) -> Option<&'static purple_garden_frontend::lex::TypeDoc> {
    purple_garden_frontend::lex::type_doc(query)
}

pub(crate) fn render_query(query: Option<&str>) -> Result<String, String> {
    let Some(query) = query else {
        return Ok(render_index());
    };

    if let Some(doc) = language_doc(query) {
        return Ok(render_language_doc(doc));
    }

    if let Some(doc) = type_doc(query) {
        return Ok(render_type_doc(doc));
    }

    let (path, method) = match query.split_once('.') {
        Some((path, method)) => (path, Some(method)),
        None => (query, None),
    };

    let Some(pkg) = pstd::resolve_pkg(path) else {
        return Err(format!("query {path} couldnt be resolved to anything"));
    };

    if let Some(method) = method {
        let Some((name, variants)) = pkg
            .overload_groups()
            .into_iter()
            .find(|(name, _)| *name == method)
        else {
            return Err(format!("function {}.{} not found", pkg.name, method));
        };

        return Ok(render_function(name, &variants));
    }

    Ok(pkg.to_string())
}

pub(crate) fn render_function(name: &str, variants: &[&purple_garden_runtime::Fn<'_>]) -> String {
    let mut out = String::new();
    purple_garden_runtime::print_overload_group(name, variants, &mut out)
        .expect("writing to a String cannot fail");
    out
}

fn render_index() -> String {
    let mut out = String::from("Purple Garden documentation\n\nPackages:\n");
    for pkg in purple_garden_std::STD {
        render_pkg_index(pkg, &mut out);
    }

    out.push_str("\nKeywords:\n");
    for doc in purple_garden_frontend::lex::KEYWORD_DOCS {
        out.push_str("  ");
        out.push_str(doc.name);
        out.push('\n');
    }

    out.push_str("\nTypes:\n");
    for doc in purple_garden_frontend::lex::TYPE_DOCS {
        out.push_str("  ");
        out.push_str(doc.name);
        out.push('\n');
    }
    out
}

fn render_pkg_index(pkg: &Pkg, out: &mut String) {
    out.push_str("  ");
    out.push_str(pkg.name);
    out.push('\n');
    for sub in pkg.pkgs {
        render_pkg_index(sub, out);
    }
}

fn render_language_doc(doc: &purple_garden_frontend::lex::KeywordDoc) -> String {
    format!("{} {}\n\n{}\n", doc.kind, doc.name, doc.doc)
}

fn render_type_doc(doc: &purple_garden_frontend::lex::TypeDoc) -> String {
    format!("type {}\n\n{}\n", doc.name, doc.doc)
}
