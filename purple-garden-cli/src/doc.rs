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

    Ok(render_package(path, pkg))
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
        render_pkg_index(pkg, None, &mut out);
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

fn render_pkg_index(pkg: &Pkg, parent: Option<&str>, out: &mut String) {
    let path = parent.map_or_else(
        || pkg.name.to_owned(),
        |parent| format!("{parent}/{}", pkg.name),
    );

    out.push_str("  ");
    out.push_str(&path);
    out.push('\n');
    for sub in pkg.pkgs {
        render_pkg_index(sub, Some(&path), out);
    }
}

fn render_package(path: &str, pkg: &Pkg) -> String {
    let mut out = format!("import (\"{path}\")\n\n");
    out.push_str(pkg.doc);
    out.push('\n');

    if !pkg.pkgs.is_empty() {
        out.push('\n');
        for sub in pkg.pkgs {
            out.push_str(path);
            out.push('/');
            out.push_str(sub.name);
            out.push('\n');
        }
    }

    if !pkg.fns.is_empty() {
        out.push('\n');
        for (name, variants) in pkg.overload_groups() {
            purple_garden_runtime::print_overload_group_summary(name, &variants, &mut out)
                .expect("writing to a String cannot fail");
        }
    }

    out
}

fn render_language_doc(doc: &purple_garden_frontend::lex::KeywordDoc) -> String {
    format!("{} {}\n\n{}\n", doc.kind, doc.name, doc.doc)
}

fn render_type_doc(doc: &purple_garden_frontend::lex::TypeDoc) -> String {
    format!("type {}\n\n{}\n", doc.name, doc.doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_prints_nested_package_paths() {
        let out = render_query(None).unwrap();

        assert!(out.contains("  unsafe\n"));
        assert!(out.contains("  unsafe/runtime\n"));
    }

    #[test]
    fn package_doc_prints_nested_package_paths() {
        let out = render_query(Some("unsafe")).unwrap();

        assert!(out.contains("import (\"unsafe\")"));
        assert!(out.contains("unsafe/runtime\n"));
        assert!(!out.contains("\nruntime\n"));
    }
}
